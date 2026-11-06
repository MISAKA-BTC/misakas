// Registry keys and payout identities survive a legal collateral withdrawal.
// Queue saturation below is a component fixture, not a forged network payout.
{
    for refund in [true,false] {
        let outsider=outsider();
        let (supplier,sponsor)=if refund{(producer,outsider)}else{(outsider,producer)};
        // Exercise a distinct sponsor and supplier on the contract under test.
        // The other roles keep their original signed contracts and wire aliases.
        let mut terms=consents[0].terms.clone();
        terms.supplier=supplier;
        terms.sponsor=sponsor;
        let distinct=sign_family_supplier_v1(terms,|who,message,context|{
            let (key,seed)=if *who==producer{(&kp.signing_key,[0x6d;32])}else{(&family_filer_key.signing_key,[0x6e;32])};
            Some(mldsa::sign(key,message,context,seed).unwrap().as_ref().to_vec())
        }).unwrap();
        let contract=distinct.id_v1().unwrap();
        let mut outer=wrapper.clone();
        outer.suppliers[0]=distinct;
        let wire=outer.encode_v2().unwrap();
        let mut carrier=pairing.carrier().clone();
        let mut ex=active.clone();
        carrier.payload=if let Some(provenance)=ex.prefix_dependencies_v1.get_mut(&claim){
            provenance.inner=wire.clone();
            provenance.encode_v1().unwrap()
        }else{wire.clone()};
        carrier.finalize();
        ex.tir_funded_originals.insert(claim,palw_funded_original_pairing_payload_v1(&carrier,&wire).unwrap());
        let (funded,_)=apply_palw_transition_v2_with_extras(&family_state,&family_params,&ctx,std::slice::from_ref(&object),None,false,false,false,false,&ex).unwrap();
        assert!(funded.claim(&claim).is_some());
        let born=fs::family_contract_for_material_v1(&funded,contract).unwrap().unwrap();
        let message=palw_bond_retirement_message_v2(network,&outsider);
        let retire=PalwConsensusObjectV2::BondRetireRequested{bond:outsider,
            signature:mldsa::sign(&family_filer_key.signing_key,message.as_byte_slice(),PALW_BOND_RETIREMENT_V2_MLDSA87_CONTEXT,[0x6e;32]).unwrap().as_ref().to_vec()};
        let retire_at=PalwBlockContextV2{block:h(0x420001),daa_score:ctx.daa_score+1,blue_score:ctx.blue_score+1,subsidy:0};
        let (retiring,_)=apply_palw_transition_v2_with_extras(&funded,&family_params,&retire_at,&[retire],None,false,false,false,false,&ex).unwrap();
        assert!(matches!(retiring.bond(&outsider).unwrap().status,PalwBondStatusV2::Retiring{..}));
        assert_eq!(retiring.bond(&outsider).unwrap().pubkey,funded.bond(&outsider).unwrap().pubkey);
        assert_eq!(retiring.bond(&outsider).unwrap().payout_payload,funded.bond(&outsider).unwrap().payout_payload);
        if refund{ex.court_service_inclusion_premise_v1=crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_INCLUSION_PREMISE_V1;}
        let mut b=TransitionBuilder::try_new(&retiring,&family_params,false,false,false,false,&ex).unwrap();
        if refund {
            let mut book=b.state.court_service_state_v1().unwrap().clone();
            let units=born.targets.iter().flat_map(|t|&t.packets).map(|id|book.book.assigned_daa(id).unwrap()).collect::<std::collections::BTreeSet<_>>();
            for unit in units{book.book.observe_block(unit,h(unit).as_bytes(),crate::palw_court_service_v1::ServiceCost::default()).unwrap();}
            // A supplied final slot remains Pending until service_before, even when the tick's
            // context is later: the supplier sweep reads the already-observed parent clock.
            let assessment=born.targets.iter().flat_map(|t|&t.packets)
                .map(|id|book.book.packets[id].service_before).max().unwrap();
            book.book.observe_block(assessment,h(assessment).as_bytes(),crate::palw_court_service_v1::ServiceCost::default()).unwrap();
            for id in born.targets.iter().flat_map(|t|&t.packets){
                assert_eq!(book.book.assess(id),crate::palw_court_service_v1::ServiceAssessment::OpportunityProvided);
            }
            if let Some(released)=b.state.court_service_row_v3_v1(){let mut row=released.clone();row.state=book;b.write_staged_row_v1(Key::CourtService,Some(Row::CourtServiceV3(Box::new(row).into()))).unwrap();}
            else{b.write_staged_row_v1(Key::CourtService,Some(Row::CourtService(Box::new(book).into()))).unwrap();}
        }
        let mut foreign=Vec::new();
        for i in 0..PALW_V2_MAX_PAYOUTS_PER_BLOCK{
            let payload=h(0x42f000+i as u64);let key=crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload);
            b.write_payout(key,Some(PalwPayoutV2{payload,amount:1})).unwrap();foreign.push(key);
        }
        let due=born.targets.iter().map(|t|t.packets.iter().map(|id|b.state.court_service_state_v1().unwrap().book.assigned_daa(id).unwrap()).max().unwrap()).min().unwrap()
            +crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.service_span_daa+1;
        let at=if refund{assert!(due<=hard);due}else{hard+1};
        if refund{assert!(b.state.court_service_state_v1().unwrap().book.last_observed_daa.unwrap()<=at,"component parent never observes a future service clock");}
        let tick=PalwBlockContextV2{block:h(at),daa_score:at,blue_score:at,subsidy:0};
        fs::tick_family_suppliers_v1(&mut b,&tick).unwrap();
        let held=fs::family_contract_for_material_v1(&b.state,contract).unwrap().unwrap();
        assert!(matches!(held.fee_state,FamilySupplierFeeStateV1::Held));
        assert_eq!(held.defaulted_daa.is_some(),refund);
        let after=born.consent.terms.material_until_daa.max(retire_at.daa_score+family_params.withdrawal_delay_daa()).max(hard)+1;
        let late=PalwBlockContextV2{block:h(after),daa_score:after,blue_score:after,subsidy:0};
        use super::super::palw_bond_budget_fold_v1::tick_bond_budget_v1;
        tick_bond_budget_v1(&mut b,&late).unwrap();
        let delay=family_params.withdrawal_delay_daa();
        let exit_held=|s:&PalwChainStateV2,at|palw_bond_collateral_is_locked_checked_v7(
            s,&family_params,&outsider,s.bond(&outsider).unwrap(),at,delay,None,true).unwrap();
        assert_eq!(b.state.family_supplier_collateral_reserved_v1(&outsider,after),0);
        // The refund recipient has no service-income reservation. The supplier's
        // R remains occupied through physical payment and the ensuing window.
        assert_eq!(exit_held(&b.state,after),!refund);
        for key in foreign{b.write_payout(key,None).unwrap();}
        let queued_state=if scoped{
            // Advance through the normal fold so expired duties, exposure
            // and the scoped checkpoint all agree with the accepting point.
            apply_palw_transition_v2_with_extras(&b.state,&family_params,&late,&[],None,false,false,false,false,&ex).unwrap().0
        }else{
            fs::tick_family_suppliers_v1(&mut b,&late).unwrap();
            super::super::palw_court_family_material_fold_v1::sweep_family_material_v1(&mut b,&late).unwrap();
            b.state.set_last_point_for_tests(Some(late));
            b.state
        };
        let queued=fs::family_contract_for_material_v1(&queued_state,contract).unwrap().unwrap();
        let FamilySupplierFeeStateV1::Queued{payee,queue,..}=queued.fee_state else{panic!("retired immutable payee remains payable after congestion")};
        assert_eq!(payee,outsider);assert!(queued_state.pending_payouts[&queue].amount>=born.consent.terms.fee_sompi);
        let bytes=borsh::to_vec(&PalwStateCarriageV2::from_state(&queued_state)).unwrap();
        let restored=borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&family_params,Some(queued_state.state_root())).unwrap();
        assert_eq!(restored.state_root(),queued_state.state_root());
        let mut paid=TransitionBuilder::try_new(&restored,&family_params,false,false,false,false,&ex).unwrap();
        let at=after+1;
        let point=PalwBlockContextV2{block:h(at),daa_score:at,blue_score:at,subsidy:0};
        super::super::block_preparation::queue_drain_workspace_v2::drain_consumed_queues_v2(&mut paid,&point).unwrap();
        tick_bond_budget_v1(&mut paid,&point).unwrap();
        assert!(matches!(fs::family_contract_for_material_v1(&paid.state,contract).unwrap().unwrap().fee_state,
            FamilySupplierFeeStateV1::Paid{payee,daa} if payee==outsider&&daa==at));
        assert_eq!(exit_held(&paid.state,at),!refund);
        if !refund{
            let at=at+family_params.bond_budget().unwrap().policy.window_daa+1;
            let point=PalwBlockContextV2{block:h(at),daa_score:at,blue_score:at,subsidy:0};
            let parent=paid.state;
            let mut released=TransitionBuilder::try_new(&parent,&family_params,false,false,false,false,&ex).unwrap();
            tick_bond_budget_v1(&mut released,&point).unwrap();
            assert!(!exit_held(&released.state,at),"paid income's finite R window has elapsed");
        }
    }
}
