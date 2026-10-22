        // G14 lane escrow-rfc08 round 2: the funded-original coordinator on the SAME real native original (written, not run).
        // Included inside `g14_native_funded_candidate_fixture_generator_v1` after the requirements block: `state`, `params`,
        // `extras`, `envelope` (the literal TFP1), `object`, `ctx`, `claim` and `fp` are that fixture's.
        {
            use crate::palw_court_original_escrow_v1::{FundingConsentV1, FundingTermsV1, COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1};
            use crate::palw_funded_original_coordinator_v1::*;
            use crate::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
            // Armed X3 mirror (test fixture policy, not a preset change) so the hard deadline is sourced.
            let x3 = crate::palw_ruling_fences_v1::PalwClaimHardDeadlineMirrorV1 {
                from_daa: 0,
                policy: crate::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at(
                    crate::config::params::ForkActivation::new(0),
                )
                .policy,
            };
            let params = params.clone().with_claim_hard_deadline_v1(Some(x3));
            let hard = crate::palw_funded_escrow_v1::palw_funded_escrow_hard_deadline_v1(Some(x3), ctx.daa_score).unwrap();
            // A court service book: the shared service namespace the complete source graph is scoped to.
            let mut served = state.clone();
            {
                use crate::palw_court_service_v1::{ServiceCost, ServiceReservations};
                let cost = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
                let service = crate::palw_court_service_state_v2::CourtServiceStateV2 {
                    version: 2,
                    source_profile_id: h(0x5E5),
                    profile_id: crate::palw_court_service_state_v2::court_service_profile_id_v2(h(0x5E5)),
                    book: ServiceReservations::new(cost).unwrap(),
                    claims: Default::default(),
                };
                served.staged_courts.insert(Key::CourtService, Row::CourtService(Box::new(service).into()));
            }
            let budgets = (
                crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3 { max_original_leaves: 1_000_000, max_roots: 1_000_000, max_paths: 10_000_000, max_edges: 10_000_000, max_path_bytes: 10_000_000 },
                crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 { max_original_leaves: 1_000_000, max_total_edges: 10_000_000, max_total_coordinates: 1_000_000, max_root_memberships: 1_000_000 },
                crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3 { max_queries: 100_000, max_roles: 8, max_vertices: 2_000_000, max_edges: 8_000_000 },
            );
            let all_roles = PALW_FUNDED_ROLE_WHOLE_BRANCH_V1 | PALW_FUNDED_ROLE_SERVICE_OPERATOR_V1
                | PALW_FUNDED_ROLE_FEE_CAPITAL_BODIES_V1 | PALW_FUNDED_ROLE_SEMANTIC_PROVIDER_V1;
            let profiles = [h(0xFEE), h(0xCA9), h(0x9E7), h(0x5E7)];
            let schedule = |roles| PalwFundedOriginalScheduleV1::test_only_v1(profiles, 700, 300, budgets.0, budgets.1, budgets.2, roles);
            // The native graph identity the payer must sign: derived here exactly as the coordinator derives it.
            let mut probe_extras = extras.clone();
            probe_extras.tir_public_originals.insert(claim, envelope.clone());
            let (graph_scope, material_until) = {
                let b = TransitionBuilder::try_new(&served, &params, false, false, false, false, &probe_extras).unwrap();
                let prepared = prepare_paired_original_v1(&b, &ctx, &object, Some(&fp.commitment)).unwrap().unwrap();
                let material = prepared.frame_v1().plan_v1().terms.material_until_daa.max(hard);
                let req = prepared.enrich_native_geometry_v1(&b, &ctx).unwrap().prepare_requirements_v1(&b, &ctx,
                    PreparedRequirementBudgetsV1 { numerical: budgets.0, input: budgets.1, source: budgets.2 }).unwrap();
                (req.source_graph_v1().unwrap().scope_v3(), material)
            };
            let payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x7A; 32]);
            let payer_pub = payer.verification_key.as_ref().to_vec();
            let refund = kaspa_hashes::blake2b_512_address_payload(&payer_pub);
            let carrier_of = |amount: u64, sign_amount: u64| -> (Transaction, PalwFundedOriginalPairingV1) {
                let terms = FundingTermsV1 {
                    version: 1,
                    network_domain: extras.tir_public_network_domain_v1.unwrap(),
                    claim,
                    job_identity: envelope.pledge.job_identity,
                    execution_root: fp.commitment.execution_root,
                    source_id: envelope.pledge.source_id,
                    relative_graph_id: graph_scope,
                    fee_profile_id: profiles[0],
                    capital_profile_id: profiles[1],
                    retention_profile_id: profiles[2],
                    settlement_policy_id: profiles[3],
                    accept_from_daa: ctx.daa_score,
                    accept_until_daa: ctx.daa_score + 10,
                    material_until_daa: material_until,
                    funding_output_index: 1,
                    funding_amount_sompi: sign_amount,
                    payer_key_id: crate::mldsa87_primitives::mldsa87_key_id(&payer_pub),
                    refund_payload: refund,
                    nonce: h(0x0CE),
                };
                let signature = libcrux_ml_dsa::ml_dsa_87::sign(&payer.signing_key, terms.id_v1().as_byte_slice(),
                    COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1, [0x7B; 32]).unwrap().as_ref().to_vec();
                let tfo1 = crate::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1::new_v1(
                    envelope.encode_v1().unwrap(), FundingConsentV1 { terms, signature }, payer_pub.clone()).unwrap();
                let tx = Transaction::new(
                    0,
                    vec![TransactionInput::new(TransactionOutpoint::new(h(0xF00D), 0), vec![], 0, 0)],
                    vec![
                        TransactionOutput::new(1, ScriptPublicKey::default()),
                        TransactionOutput::new(amount, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&refund.as_bytes())),
                    ],
                    0,
                    crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
                    0,
                    tfo1.encode_v1().unwrap(),
                );
                let pairing = palw_funded_original_pairing_v1(&tx).unwrap();
                (tx, pairing)
            };
            let (_carrier, pairing) = carrier_of(1_000, 1_000);
            let with = |sched: Option<PalwFundedOriginalScheduleV1>, pairing: &PalwFundedOriginalPairingV1| {
                let mut e = probe_extras.clone();
                e.tir_funded_originals.insert(claim, pairing.clone());
                e.funded_original_test_schedule_v1 = sched;
                e
            };
            let run = |st: &PalwChainStateV2, e: &PalwTransitionExtrasV1| {
                let b = TransitionBuilder::try_new(st, &params, false, false, false, false, e).unwrap();
                let Some(prepared) = prepare_paired_original_v1(&b, &ctx, &object, Some(&fp.commitment)).ok().flatten() else {
                    return Err(FundedOriginalRefusalV1::Invalid("native preparation refused"));
                };
                coordinate_funded_original_v1(&b, &ctx, claim, prepared)
            };

            // 1. Production: no schedule value can exist → typed-missing Schedule; nothing is written.
            let prod = with(None, &pairing);
            assert!(matches!(run(&served, &prod), Err(FundedOriginalRefusalV1::Missing(PalwFundedOriginalMissingV1::Schedule))));
            {
                let b = TransitionBuilder::try_new(&served, &params, false, false, false, false, &prod).unwrap();
                let before = b.state.state_root();
                let prepared = prepare_paired_original_v1(&b, &ctx, &object, Some(&fp.commitment)).unwrap();
                assert!(admit_original_v1(&b, &ctx, claim, prepared).is_err());
                assert_eq!(b.state.state_root(), before);
                assert!(b.state.funded_escrows_v1().is_empty());
                // A funded pairing never rides in as an unfunded ordinary TFP1 claim.
                assert!(admit_original_v1(&b, &ctx, claim, None).is_err());
            }
            // 2. Schedule present but a role contract not adopted → that typed reason.
            assert!(matches!(run(&served, &with(Some(schedule(0)), &pairing)),
                Err(FundedOriginalRefusalV1::Missing(PalwFundedOriginalMissingV1::CanonicalWholeBranchActorAssignment))));
            // 3. No service book → typed-missing SharedServiceNamespace.
            assert!(matches!(run(&state, &with(Some(schedule(all_roles)), &pairing)),
                Err(FundedOriginalRefusalV1::Missing(PalwFundedOriginalMissingV1::SharedServiceNamespace))));
            // 4. Success: every value present → the row binds the actual outpoint, amount, payer and derived hard deadline.
            let ok = with(Some(schedule(all_roles)), &pairing);
            let plan = run(&served, &ok).unwrap();
            let row = *plan.row_v1();
            assert_eq!(row.claim, claim);
            assert_eq!(row.funding_outpoint, pairing.outpoint());
            assert_eq!(row.amount_sompi, 1_000);
            assert_eq!(row.refund_payload, refund);
            assert_eq!(row.accepted_daa, ctx.daa_score);
            assert_eq!(row.hard_deadline_daa, hard);
            assert_eq!(row.contract_id, pairing.envelope().funding.terms.id_v1());
            assert!(!row.liability_open && row.settled_by.is_none());
            drop(plan);
            // 5. Funding below fee+capital (1,000) is refused, even though the payer signed and the output matches its terms.
            let (_, short) = carrier_of(999, 999);
            assert!(matches!(run(&served, &with(Some(schedule(all_roles)), &short)), Err(FundedOriginalRefusalV1::Invalid(_))));
            // 6. A schedule profile the payer did not sign (caller profile hash is not policy) → refused.
            let other = PalwFundedOriginalScheduleV1::test_only_v1([h(1), profiles[1], profiles[2], profiles[3]], 700, 300, budgets.0, budgets.1, budgets.2, all_roles);
            assert!(matches!(run(&served, &with(Some(other), &pairing)), Err(FundedOriginalRefusalV1::Invalid(_))));
            // 7. The current producer authority changed after signing → refused by the actor catalog.
            {
                let mut rotated = served.clone();
                rotated.bonds.get_mut(&PalwBondKeyV2(fp.commitment.job.executor_bond)).unwrap().operator_id = h(0xBAD);
                assert!(run(&rotated, &ok).is_err());
            }
            // 8. Atomicity: the escrow row installed inside a checkpoint vanishes with every other write when the object fails.
            {
                let mut b = TransitionBuilder::try_new(&served, &params, false, false, false, false, &ok).unwrap();
                let before = b.state.state_root();
                let entries = b.entries.len();
                let saved = b.try_checkpoint().unwrap();
                b.install_funded_escrow_v1(row).unwrap();
                assert_eq!(b.state.funded_escrows_v1().len(), 1);
                b.restore(saved);
                assert_eq!(b.state.state_root(), before);
                assert_eq!(b.entries.len(), entries);
                assert!(b.state.funded_escrows_v1().is_empty());
            }
        }
