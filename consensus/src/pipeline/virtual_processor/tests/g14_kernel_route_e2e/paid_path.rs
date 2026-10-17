//! **Approved paid claims as beacon sources, on any harness network** (ADR-0185; the `Net`-level twin of the conformance world's
//! `paid_sources`). Since the RFC07 adapter an OPV timeout Final exports no beacon event: the one source of an event is a `PaidClaimV2`
//! Final the assigned quorum approved. These helpers drive that path through the fold, each step signed by the actor the fold names,
//! with no seam: the registrant's paid class over the source class, the payer's job enrollment (before any seal of the job), the
//! producer's signed covenant over the claim's complete served material, the paid commit (the fold applies the salted reveal inside —
//! so a sealed source's reveal IS its paid commit), the lock (the pinned bootstrap draw while no approved paid Final exists) and the
//! drawn seats' receipts. The claim then reaches its canonical paid Final on its own clock.
use super::*;
use kaspa_consensus_core::palw_claim_challenge_v2::{PAID_CLAIM_LOCKS_V2, PalwPaidClaimAuthorityV2, PalwPaidClaimLockV2};
use kaspa_consensus_core::palw_monetary_material_v1::{
    PalwMonetaryMaterialEnrollmentV1, PalwMonetaryMaterialOfferV1, PalwMonetaryMaterialTermsV1,
};
use kaspa_consensus_core::palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1, palw_provider_court_message_v1};
use misaka_palw_kernel::claim_paid_v2::{PaidClaimObjectV2, PaidClaimReceiptV2};

pub(super) use super::conformance::paid_sources::arm_paid_sources;

fn card_sign(card: usize, message: &[u8], context: &[u8], rnd: u8) -> Vec<u8> {
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [rnd; 32]).expect("ML-DSA-87 signs").as_ref().to_vec()
}

/// One sealed claim of a source job, kept for its paid reveal: the producer card, its salted reveal, its id and its served trace.
pub(super) struct SealedPaid {
    pub card: usize,
    pub reveal: K,
    pub id: Digest,
    pub trace: TraceV1,
    pub paid_job: Digest,
}

impl Net {
    fn bond_card(&self, bond: &PalwBondKeyV2) -> usize {
        (0..self.chain.bonds.len()).find(|c| self.bond(*c) == *bond).expect("a harness card")
    }

    /// The registrant's paid verification class over `base_class`.
    pub(super) async fn paid_class_over(&mut self, registrant: usize, base_class: Digest) -> Digest {
        let source = self.config.params.palw_beacon_source_opt_in_v1.expect("the source fence").policy;
        let policy = self.config.params.palw_claim_challenge_v2.expect("the paid path is armed").policy;
        let route = self.api().expect("the route");
        let object = misaka_palw_sdk::claim_paid_v2::paid_claim_class_object_v2(
            &route,
            &policy,
            self.kid(registrant),
            base_class,
            misaka_palw_challenge::paid_opt_in_source_policy_v5(2, 2, 20, 2, 1),
            &source,
        )
        .expect("the paid class over the base class");
        let K::PaidClaimV2 { object: PaidClaimObjectV2::RegisterClass { class } } = &object else { unreachable!() };
        let id = class.id();
        let o = self.route(registrant, &object);
        self.send(vec![(registrant, o)]).await;
        assert!(
            self.api()
                .unwrap()
                .aux_row::<misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2>(
                    kaspa_consensus_core::palw_claim_challenge_v2::PAID_CLAIM_CLASSES_V2,
                    &borsh::to_vec(&id).unwrap()
                )
                .is_some(),
            "the registrant's paid class is enrolled"
        );
        id
    }

    /// The payers' prospective consents for their `jobs` under `paid_class` — before any seal of the jobs.
    pub(super) async fn enroll_paid_jobs(&mut self, paid_class: Digest, jobs: &[(usize, Digest)]) -> Vec<Digest> {
        let route = self.api().unwrap();
        let mut objects = Vec::new();
        let mut ids = Vec::new();
        for (payer, job) in jobs {
            let enroll = misaka_palw_sdk::claim_paid_v2::paid_claim_job_object_v2(
                &route,
                paid_class,
                *job,
                self.kid(*payer),
                misaka_palw_kernel::hash::object_id(b"MISAKA/TEST/PAID-JOB-NONCE", job),
                self.daa() + 400,
                1,
            )
            .unwrap_or_else(|e| panic!("the payer enrolls its job: {e:?}"));
            let K::PaidClaimV2 { object: PaidClaimObjectV2::EnrollJob { job: paid_job } } = &enroll else { unreachable!() };
            ids.push(paid_job.id());
            objects.push((*payer, self.route(*payer, &enroll)));
        }
        self.send(objects).await;
        for id in &ids {
            assert!(
                self.api().unwrap().aux.keys().any(|(t, k)| *t == kaspa_consensus_core::palw_claim_challenge_v2::PAID_CLAIM_JOBS_V2
                    && *k == borsh::to_vec(id).unwrap()),
                "the payer's job enrollment landed"
            );
        }
        ids
    }

    /// The producer's signed monetary covenant over the claim `base` whose exact commit object is `reveal` — the complete served
    /// material of every committed position, verified against the ledger that admits the claim, two independent providers.
    pub(super) async fn kernel_covenant(&mut self, producer: usize, base: Digest, trace: &TraceV1, reveal: &K) -> Hash64 {
        let route = self.api().unwrap();
        let mut admitted: misaka_palw_kernel::ledger::KernelLedgerV1 = (*route.ledger().expect("the route's ledger")).clone();
        admitted.daa = self.daa() + 1;
        admitted
            .apply_object(reveal, &misaka_palw_kernel::ledger::AuthV1 { signer_bond: self.kid(producer) })
            .unwrap_or_else(|e| panic!("the claim is admissible: {e:?}"));
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
                                if mask[st][n] { MaterialResponseV1::Omitted } else { MaterialResponseV1::Whole(TensorWireV1::of(t)) }
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
        let bundle =
            kaspa_consensus_core::palw_material_capacity_v1::PalwKernelMaterialBundleV1 { claim: Hash64::from_bytes(base), positions };
        let scope = bundle.verify(&admitted, u64::MAX).unwrap_or_else(|e| panic!("the complete served material verifies: {e:?}"));
        let manifest =
            kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialManifestV1 { root: Hash64::from_bytes(base), scopes: vec![scope] };
        let bytes = manifest.scopes[0].bytes + 4_096;
        let policy = self.config.params.palw_monetary_material_v1.as_ref().expect("the covenant fence").policy.clone();
        let state = self.chain.tip_state().1;
        let own = state.bond(&self.bond(producer)).unwrap().operator_id;
        let mut operators = std::collections::BTreeSet::from([own]);
        let providers: Vec<usize> = (0..self.chain.bonds.len())
            .filter(|c| *c != producer && operators.insert(state.bond(&self.bond(*c)).unwrap().operator_id))
            .take(2)
            .collect();
        let root = Hash64::from_bytes(base);
        let mut enrollment = PalwMonetaryMaterialEnrollmentV1 {
            terms: PalwMonetaryMaterialTermsV1 {
                version: 1,
                root,
                dependencies: Vec::new(),
                exec_plan_root: None,
                sponsor: self.bond(producer),
                nonce: Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/TEST/COVENANT-NONCE", &base)),
                policy: policy.id(),
                max_material_bytes: bytes,
                material_manifest: Some(manifest),
                native_session: None,
                window_daa: policy.window_daa,
                enrollment_until_daa: self.daa() + 100,
                payment_relative_until_finality: true,
            },
            offers: providers
                .iter()
                .map(|card| PalwMonetaryMaterialOfferV1 {
                    provider: self.bond(*card),
                    collateral_sompi: policy.minimum_collateral_sompi,
                    prepaid_fee_sompi: policy.minimum_fee_sompi,
                    signature: Vec::new(),
                })
                .collect(),
        };
        enrollment.offers.sort_by_key(|o| o.provider);
        let payload = enrollment.payload();
        for offer in &mut enrollment.offers {
            let card = self.bond_card(&offer.provider);
            let message = palw_provider_court_message_v1(self.domain, 240, &offer.provider, &payload);
            offer.signature = card_sign(card, message.as_byte_slice(), PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1, 0xD1);
        }
        let message = palw_provider_court_message_v1(self.domain, 240, &enrollment.terms.sponsor, &payload);
        let object = Obj::MonetaryMaterialEnrolledV1 {
            enrollment: Box::new(enrollment),
            signature: card_sign(producer, message.as_byte_slice(), PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1, 0xD2),
        };
        let rehearsal = self.chain.ctx.consensus.palw_object_rehearsal_v1(&object);
        self.send(vec![(producer, object)]).await;
        self.chain
            .tip_state()
            .1
            .monetary_material_row_v1(root)
            .unwrap_or_else(|| panic!("the producer's signed covenant is enrolled (rehearsal: {rehearsal:?})"))
            .consent
    }

    /// **Seal honest claims of `jobs` (class `class`, weights `fx`) by `producers`, kept for their paid reveal.** The seals are
    /// ordinary salted seals (the sealed-source beacons read them); nothing is revealed yet.
    pub(super) async fn seal_paid(
        &mut self,
        fx: &Fixture,
        class: Digest,
        jobs: &[KernelJobV1],
        paid_jobs: &[Digest],
        producers: &[usize],
    ) -> Vec<SealedPaid> {
        let ledger = self.ledger();
        let mut seals = Vec::new();
        let mut out = Vec::new();
        for ((job, paid_job), p) in jobs.iter().zip(paid_jobs).zip(producers) {
            let generated = greedy(fx, &ledger, &class, &job.prompt, job.max_new_tokens as usize);
            let produced = produce(fx, &ledger, &class, job, self.kid(*p), generated, |_| {});
            let (seal, reveal) = seal_and_reveal(&ledger, self.kid(*p), &produced.object);
            assert!(matches!(reveal, K::CommitClaimSalted { .. }), "past the fence the reveal carries its salt");
            seals.push((*p, self.route(*p, &seal)));
            out.push(SealedPaid { card: *p, reveal, id: produced.claim.id(), trace: produced.trace, paid_job: *paid_job });
        }
        self.send(seals).await;
        out
    }

    /// **Reveal each sealed claim as its paid commit**: the producer's covenant, then the paid commit. Returns `(base, enrollment)`.
    pub(super) async fn reveal_paid(&mut self, paid_class: Digest, sealed: &[SealedPaid]) -> Vec<(Digest, Digest)> {
        let mut consents = Vec::new();
        for s in sealed {
            consents.push(self.kernel_covenant(s.card, s.id, &s.trace, &s.reveal).await);
        }
        let route = self.api().unwrap();
        let mut objects = Vec::new();
        let mut out = Vec::new();
        for (s, consent) in sealed.iter().zip(consents) {
            let K::CommitClaimSalted { salt, commit } = s.reveal.clone() else { unreachable!() };
            let paid = misaka_palw_sdk::claim_paid_v2::paid_claim_commit_object_derived_v2(
                &route,
                paid_class,
                s.paid_job,
                commit,
                salt,
                consent.as_bytes(),
                self.daa(),
            )
            .unwrap_or_else(|e| panic!("the producer's paid commit: {e:?}"));
            let K::PaidClaimV2 { object: PaidClaimObjectV2::Commit { enrollment, .. } } = &paid else { unreachable!() };
            out.push((s.id, enrollment.id()));
            objects.push((s.card, self.route(s.card, &paid)));
        }
        for ((card, object), (base, _)) in objects.into_iter().zip(&out) {
            let rehearsal = self.chain.ctx.consensus.palw_object_rehearsal_v1(&object);
            let from = self.daa();
            self.send(vec![(card, object)]).await;
            let ledger = self.ledger();
            assert!(
                ledger.claims.contains_key(base),
                "the paid commit admitted the base claim (its salted reveal) in DAA {from}..{} (rehearsal {rehearsal:?}; seals {}, claims {})",
                self.daa(),
                ledger.seals.len(),
                ledger.claims.len()
            );
        }
        out
    }

    /// **The lock and the drawn seats' receipts** of each paid claim (the lock signed by `locker`; one receipt a block).
    pub(super) async fn lock_and_receipt(&mut self, locker: usize, claims: &[(Digest, Digest)]) {
        for (_, enrollment) in claims {
            let lock = K::PaidClaimV2 { object: PaidClaimObjectV2::Lock { enrollment: *enrollment } };
            let o = self.route(locker, &lock);
            let rehearsal = self.chain.ctx.consensus.palw_object_rehearsal_v1(&o);
            self.send(vec![(locker, o)]).await;
            assert!(
                self.api().unwrap().aux.contains_key(&(PAID_CLAIM_LOCKS_V2, borsh::to_vec(enrollment).unwrap())),
                "the lock is taken (rehearsal: {rehearsal:?})"
            );
        }
        for (base, enrollment) in claims {
            let route = self.api().unwrap();
            let lock: PalwPaidClaimLockV2 = route.aux_row(PAID_CLAIM_LOCKS_V2, &borsh::to_vec(enrollment).unwrap()).expect("the lock");
            let authority: PalwPaidClaimAuthorityV2 = route.paid_claim_authority_for_base_unchecked_v2(base).expect("the paid authority");
            for seat in &lock.seats {
                let card = self.bond_card(seat);
                let receipt = PaidClaimReceiptV2 {
                    version: 2,
                    enrollment: *enrollment,
                    verification_class: authority.class.id(),
                    assignment: lock.assignment,
                    source_context: lock.context,
                    sample_seed: lock.seed,
                    evidence_root: authority.enrollment.commit_root,
                    transcript_root: authority.enrollment.prior_transcript,
                    signer: self.kid(card),
                    signed_daa: self.daa(),
                    complete: true,
                    round: 0,
                    message_root: [0; 64],
                };
                let o = self.route(card, &K::PaidClaimV2 { object: PaidClaimObjectV2::Receipt { receipt } });
                self.send(vec![(card, o)]).await;
            }
            assert!(
                self.ledger().paid_claim_consumers_v2.get(base).is_some_and(|c| c.complete),
                "the assigned quorum's receipts complete the paid claim"
            );
        }
    }

    /// **Approved paid claims of `class`, end to end**: each payer posts and enrolls a job (under the registrant's paid class over
    /// `class`, registered once per call), the matching producer seals an honest claim of it and reveals it as its paid commit under its
    /// signed covenant, and the lock and the drawn seats' receipts follow. Returns the base claim ids; they reach paid Final on their
    /// own clock (≈ 444 DAA under the probe policies) and only then export a beacon event.
    pub(super) async fn paid_claims(
        &mut self,
        registrant: usize,
        fx: &Fixture,
        class: Digest,
        payers: &[usize],
        producers: &[usize],
        nonce: u8,
        source: Option<(&Fixture, Digest)>,
    ) -> Vec<Digest> {
        assert_eq!(payers.len(), producers.len());
        let jobs: Vec<KernelJobV1> = (0..payers.len())
            .map(|i| KernelJobV1 {
                class_binding_id: class,
                prompt: vec![3, 17, 9],
                max_new_tokens: 3,
                decode: DecodeRuleV1::Greedy,
                nonce: [nonce.wrapping_add(i as u8); 64],
            })
            .collect();
        let posts: Vec<(usize, Obj)> =
            jobs.iter().zip(payers).map(|(j, &payer)| (payer, self.route(payer, &K::PostJob { job: j.clone() }))).collect();
        self.send(posts).await;
        let paid_class = self.paid_class_over(registrant, class).await;
        let pairs: Vec<(usize, Digest)> = payers.iter().copied().zip(jobs.iter().map(|j| j.id())).collect();
        let paid_jobs = self.enroll_paid_jobs(paid_class, &pairs).await;
        let sealed = self.seal_paid(fx, class, &jobs, &paid_jobs, producers).await;
        let paid = self.reveal_paid(paid_class, &sealed).await;
        // ADR-0185: once an approved paid Final exists the pinned bootstrap draw is closed; each later target locks on the seal-reveal
        // seed of sources sealed after its commitment (the caller names the eligible source class and its weights).
        if self.api().unwrap().paid_claim_approved_final_exists_v2() {
            let (fx, source_class) = source.expect("a later paid claim needs seal-reveal sources of an eligible class");
            for ((base, enrollment), (payer, producer)) in paid.iter().zip(payers.iter().zip(producers)) {
                self.seal_reveal_sources(*base, *enrollment, *payer, *producer, fx, source_class, nonce.wrapping_add(0x80)).await;
            }
        }
        self.lock_and_receipt(registrant, &paid).await;
        paid.into_iter().map(|(base, _)| base).collect()
    }

    /// The producers that are a live source target's producer (RFC07-3: such a producer seals as a source only before that
    /// target's commitment): every paid target with a beacon source row inside its signed horizon.
    pub(super) fn live_target_producers(&self) -> std::collections::BTreeSet<Digest> {
        let route = self.api().unwrap();
        let ledger = self.ledger();
        let now = self.daa();
        ledger
            .beacon_sources
            .iter()
            .filter(|(_, row)| now <= row.terms.hold_until)
            .filter_map(|((context, _), _)| route.paid_claim_authority_v2(context).ok().map(|a| a.enrollment.producer))
            .collect()
    }

    /// **The seal-reveal seed of a later paid target** (ADR-0185 F1, once an approved paid Final exists): the payer `sponsor` funds
    /// the target's future source fee before its seal window; `k` bonds other than the target's producer, the sponsor and any live
    /// source target's producer seal honest claims of fresh jobs of the eligible `source_class` inside the window and opt them in as
    /// paid sources; after the seal window each reveals under its signed covenant; returns at the reveal window's end (the lock then
    /// takes the seal-reveal draw). Nothing here is Final.
    pub(super) async fn seal_reveal_sources(
        &mut self,
        base: Digest,
        enrollment: Digest,
        sponsor: usize,
        target_producer: usize,
        source_fx: &Fixture,
        source_class: Digest,
        nonce: u8,
    ) {
        use misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTermsV1;
        let fee_policy = self.config.params.palw_beacon_source_fee_v1.expect("the source fee fence").policy;
        let route = self.api().unwrap();
        let authority: PalwPaidClaimAuthorityV2 = route.paid_claim_authority_for_base_unchecked_v2(&base).expect("the paid authority");
        let target = authority.source_target;
        assert!(target.context.eligible_profiles.contains(&source_class), "the source class is an eligible source profile");
        let fee = fee_policy.minimum_fee_for(&target).expect("the frozen source target's fee floor");
        let k = authority.class.source_policy.work_count_k;
        let fund =
            misaka_palw_sdk::claim_paid_v2::paid_claim_funding_object_v2(&route, enrollment, self.kid(sponsor), fee, k, &fee_policy)
                .unwrap_or_else(|e| panic!("the payer funds the future sources: {e:?}"));
        let o = self.route(sponsor, &fund);
        self.send(vec![(sponsor, o)]).await;
        let terms = BeaconSourceFeeTermsV1 { source: target.terms, sponsor: self.kid(sponsor), fee_per_source: fee, max_sources: k };
        assert!(self.ledger().beacon_source_fees.contains_key(&terms.source.context), "the future source fee is funded");
        let barred = self.live_target_producers();
        let target_kid = self.kid(target_producer);
        let sealers: Vec<usize> = (0..self.chain.bonds.len())
            .filter(|c| *c != target_producer && *c != sponsor && !barred.contains(&self.kid(*c)) && self.kid(*c) != target_kid)
            .take(k as usize)
            .collect();
        assert_eq!(sealers.len(), k as usize, "enough bonds that are no live source target's producer");
        let payers: Vec<usize> = (0..self.chain.bonds.len()).filter(|c| !sealers.contains(c)).take(k as usize).collect();
        let mut jobs = Vec::new();
        for (i, payer) in payers.iter().enumerate() {
            let job = KernelJobV1 {
                class_binding_id: source_class,
                prompt: vec![5, 11, 2],
                max_new_tokens: 3,
                decode: DecodeRuleV1::Greedy,
                nonce: [nonce.wrapping_add(i as u8); 64],
            };
            let o = self.route(*payer, &K::PostJob { job: job.clone() });
            self.send(vec![(*payer, o)]).await;
            jobs.push(job);
        }
        if self.daa() < terms.source.seal_start {
            self.beat_to(terms.source.seal_start).await;
        }
        let ledger = self.ledger();
        let mut seals = Vec::new();
        let mut reveals = Vec::new();
        for (job, p) in jobs.iter().zip(&sealers) {
            let generated = greedy(source_fx, &ledger, &source_class, &job.prompt, job.max_new_tokens as usize);
            let produced = produce(source_fx, &ledger, &source_class, job, self.kid(*p), generated, |_| {});
            let (seal, reveal) = seal_and_reveal(&ledger, self.kid(*p), &produced.object);
            seals.push((*p, self.route(*p, &seal)));
            reveals.push((*p, produced.claim.id(), produced.trace, reveal));
        }
        self.send(seals).await;
        let sealed = self.ledger();
        let mut opt_ins = Vec::new();
        for (job, p) in jobs.iter().zip(&sealers) {
            let producer = self.kid(*p);
            let seal = sealed.seals[&(job.id(), producer)].seal;
            let object = K::OptInPaidBeaconSource {
                producer,
                job: job.id(),
                seal,
                context: terms.source.context,
                source_terms: terms.source.id(),
                fee_terms: terms.id(),
            };
            opt_ins.push((*p, self.route(*p, &object)));
        }
        self.send(opt_ins).await;
        for p in &sealers {
            assert!(
                self.ledger().beacon_source_fee_promises.contains_key(&(terms.source.context, self.kid(*p))),
                "the sealed source opted in with its prepaid fee promise"
            );
        }
        self.beat_to(terms.source.seal_end).await;
        for (p, id, trace, reveal) in &reveals {
            self.kernel_covenant(*p, *id, trace, reveal).await;
        }
        let carriers: Vec<_> = reveals.iter().map(|(p, _, _, r)| (*p, self.route(*p, r))).collect();
        self.send(carriers).await;
        for (_, id, _, _) in &reveals {
            assert!(self.ledger().claims.contains_key(id), "the opted-in source revealed inside its window");
        }
        self.beat_to(terms.source.reveal_end).await;
    }
}
