{
// Actual family roles remain unchanged. Missing ordinary numerical operands
// are separately paid by the producer through existing signed tag-255 intake.
// This is data transport, never a new pledge or a source-duty completion.
use crate::palw_court_shared_material_intake_v1 as paid;
use crate::palw_court_shared_material_intake_v1::{MaterialActionV1 as Action, MaterialTargetV1 as Target};
let mut supplied = restored.clone();
let native_context = crate::palw_court_shared_tir_state_v1::context(&supplied, claim).unwrap().clone();
let mut missing = Vec::new();
if !matches!(supplied.staged_court_row_v1(&Key::CourtSharedIds(claim)), Some(Row::CourtSharedIds(_))) {
    let mut bytes = Vec::new();
    for offset in (0..u32::try_from(fx.execution.generated.len()).unwrap()).step_by(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize) {
        bytes.extend_from_slice(&borsh::to_vec(&response(Q::Ids {offset})).unwrap());
    }
    missing.push((Target::FullIds, bytes));
}
for ordinal in 0..requests.len() {
    let query = requests.request_at(ordinal).unwrap();
    if matches!(query, Q::Ids {..}) {continue;}
    let target = Target::Unit {ordinal};
    if matches!(supplied.staged_court_row_v1(&Key::CourtMaterial(claim,target.id_v1(claim).unwrap())), Some(Row::CourtMaterial(_))) {continue;}
    if matches!(supplied.staged_court_row_v1(&Key::CourtSharedQuery(requests.request_id(ordinal).unwrap())), Some(Row::CourtSharedQuery(q)) if q.body.is_some()) {continue;}
    missing.push((target, borsh::to_vec(&response(query)).unwrap()));
}
let schedule = paid::material_intake_schedule_v1().unwrap();
assert!(schedule.sompi_per_byte > 0, "the actual adopted paid storage schedule must apply");
let mut checked_bad_signature = false;
for (target, body) in missing {
    let collateral = supplied.bond(&producer).unwrap().collateral;
    let slashed = supplied.bond(&producer).unwrap().slashed;
    let previous_charged = match supplied.staged_court_row_v1(&Key::CourtMaterialLedger(claim,producer)) {
        Some(Row::CourtMaterialLedger(row)) => row.charged_sompi, _ => 0,
    };
    let total_bytes = u64::try_from(body.len()).unwrap();
    let fee = total_bytes.checked_mul(schedule.sompi_per_byte).unwrap();
    let digest = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body);
    let draft = paid::material_draft_id_v1(claim,target,producer,total_bytes,digest).unwrap();
    let mut actions = vec![Action::Begin {total_bytes,body_hash:digest}];
    for (part,bytes) in body.chunks(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).enumerate() {
        actions.push(Action::Part {draft_id:draft,offset:(part*crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1) as u64,bytes:bytes.to_vec()});
    }
    actions.push(Action::Finish {draft_id:draft});
    for action in actions {
        let mut signed = paid::SignedMaterialIntakeV1 {version:1,claim,scope_id:native_context.scope_id,target,submitter:producer,action,signature:vec![]};
        signed.signature = mldsa::sign(&kp.signing_key,signed.signing_message(network).unwrap().as_byte_slice(),paid::MATERIAL_INTAKE_CONTEXT_V1,seed).unwrap().as_ref().to_vec();
        elapsed += 1;
        let last = *supplied.last_point().unwrap();
        let point = PalwBlockContextV2 {daa_score:last.daa_score.checked_add(1).unwrap(),blue_score:last.blue_score.checked_add(1).unwrap(),block:h(0x450000+elapsed),subsidy:0};
        assert!(point.daa_score < native_context.hard_daa && point.daa_score <= native_context.material_until_daa,
            "actual positive paid intake must fit the unchanged signed horizons");
        let object = PalwConsensusObjectV2::MaterialIntakeV1 {intake:Box::new(signed.clone())};
        if !checked_bad_signature {
            let mut corrupt = signed; corrupt.signature[0] ^= 1;
            let before = supplied.state_root();
            assert!(apply_palw_transition_v2_with_extras(&supplied,&family_params,&point,
                &[PalwConsensusObjectV2::MaterialIntakeV1 {intake:Box::new(corrupt)}],None,false,false,false,false,&active).is_err());
            assert_eq!(supplied.state_root(),before,"invalid actual signature cannot burn or write material");
            checked_bad_signature = true;
        }
        let previous = supplied.clone();
        let (next,delta) = apply_palw_transition_v2_with_extras(&supplied,&family_params,&point,&[object],None,false,false,false,false,&active).unwrap();
        let mut undone = next.clone();
        for entry in delta.entries.iter().rev() {apply_delta_entry(&mut undone,entry,true).unwrap();}
        undone.set_last_point_for_tests(previous.last_point().cloned());
        assert_eq!(undone.state_root(),previous.state_root(),"actual paid intake delta reverses fee/native burn and every material owner");
        supplied = next;
    }
    assert_eq!(supplied.bond(&producer).unwrap().collateral,collateral.checked_sub(fee).unwrap());
    assert_eq!(supplied.bond(&producer).unwrap().slashed,slashed.checked_add(fee).unwrap());
    let Some(Row::CourtMaterialLedger(ledger)) = supplied.staged_court_row_v1(&Key::CourtMaterialLedger(claim,producer)) else {panic!("actual positive paid-intake ledger")};
    assert_eq!(ledger.charged_sompi,previous_charged.checked_add(fee).unwrap());
    assert_eq!(ledger.reserved_bytes,0);
    assert!(supplied.staged_court_row_v1(&Key::CourtMaterialDraft(claim,draft)).is_none());
    match target {
        Target::FullIds => assert!(matches!(supplied.staged_court_row_v1(&Key::CourtSharedIds(claim)), Some(Row::CourtSharedIds(_)))),
        Target::Unit {..} => assert!(matches!(supplied.staged_court_row_v1(&Key::CourtMaterial(claim,target.id_v1(claim).unwrap())),Some(Row::CourtMaterial(row)) if row.bytes == body)),
        _ => unreachable!(),
    }
}
let encoded = borsh::to_vec(&PalwStateCarriageV2::from_state(&supplied)).unwrap();
let loaded = borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state(&family_params,Some(supplied.state_root())).unwrap();
assert_eq!(loaded.state_root(),supplied.state_root());
let owner = crate::palw_court_rooted_numerical_material_v1::RootedNumericalMaterialV1::retain_v1(&loaded,claim).unwrap();
let store = owner.store_v1(&loaded,claim,native_context.scope_id).unwrap();
assert!(store.decode_pin().is_some());
for leaf in 0..fx.execution.binding.step_leaf_count {
    assert!(store.step_leaf(leaf).is_some(),"actual signed paid Unit leaf reaches cold rooted store");
    assert!(store.step_opening(leaf).is_some(),"actual signed paid Unit opening reaches cold rooted store");
}
drop(store); drop(owner);
loaded
}
