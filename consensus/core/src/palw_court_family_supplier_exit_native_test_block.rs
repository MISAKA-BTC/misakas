// Registry keys and payout identities survive a legal collateral withdrawal.
// Queue saturation below is a component fixture, not a forged network payout.
{
    for refund in [true,false] {
        let outsider=outsider();
        let (supplier,sponsor)=if refund{(producer,outsider)}else{(outsider,producer)};
        let distinct=consents.iter().map(|c|{
            let mut terms=c.terms.clone();terms.supplier=supplier;terms.sponsor=sponsor;
            sign_family_supplier_v1(terms,|who,message,context|{
                let (key,seed)=if *who==producer{(&kp.signing_key,[0x6d;32])}else{(&family_filer_key.signing_key,[0x6e;32])};
                Some(mldsa::sign(key,message,context,seed).unwrap().as_ref().to_vec())
            }).unwrap()
        }).collect::<Vec<_>>();
        let contract=distinct[0].id_v1().unwrap();
        let mut outer=wrapper.clone();outer.suppliers=distinct;
        let mut carrier=pairing.carrier().clone();carrier.payload=outer.encode_v2().unwrap();
        let mut ex=active.clone();ex.tir_funded_originals.insert(claim,palw_funded_original_pairing_v1(&carrier).unwrap());
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
        let tick=PalwBlockContextV2{block:h(at),daa_score:at,blue_score:at,subsidy:0};
        fs::tick_family_suppliers_v1(&mut b,&tick).unwrap();
        let held=fs::family_contract_for_material_v1(&b.state,contract).unwrap().unwrap();
        assert!(matches!(held.fee_state,FamilySupplierFeeStateV1::Held));
        assert_eq!(held.defaulted_daa.is_some(),refund);
        let after=born.consent.terms.material_until_daa.max(retire_at.daa_score+family_params.withdrawal_delay_daa()).max(hard)+1;
        let late=PalwBlockContextV2{block:h(after),daa_score:after,blue_score:after,subsidy:0};
        super::super::palw_bond_budget_fold_v1::tick_bond_budget_v1(&mut b,&late).unwrap();
        // Actual v6 exit predicate used by node BondSpendFilter, not a new
        // indefinite sponsor/provider hold invented to save the fee lookup.
        assert!(!palw_bond_collateral_is_locked_v6(&b.state,&family_params,&outsider,b.state.bond(&outsider).unwrap(),after,family_params.withdrawal_delay_daa(),None,true));
        assert!(b.state.bond(&outsider).is_some(),"append-only registry survives legal UTXO withdrawal eligibility");
        for key in foreign{b.write_payout(key,None).unwrap();}
        fs::tick_family_suppliers_v1(&mut b,&late).unwrap();
        super::super::palw_court_family_material_fold_v1::sweep_family_material_v1(&mut b,&late).unwrap();
        b.state.set_last_point_for_tests(Some(late));
        let queued=fs::family_contract_for_material_v1(&b.state,contract).unwrap().unwrap();
        let FamilySupplierFeeStateV1::Queued{payee,queue,..}=queued.fee_state else{panic!("retired immutable payee remains payable after congestion")};
        assert_eq!(payee,outsider);assert!(b.state.pending_payouts[&queue].amount>=born.consent.terms.fee_sompi);
        let bytes=borsh::to_vec(&PalwStateCarriageV2::from_state(&b.state)).unwrap();
        let restored=borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&family_params,Some(b.state.state_root())).unwrap();
        assert_eq!(restored.state_root(),b.state.state_root());
    }
}
