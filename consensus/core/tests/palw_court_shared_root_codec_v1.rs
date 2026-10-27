//! Native typed/wire shape fixture only; not original/funding/signature admission.
#[path = "palw_tir_fixture_common.rs"]
mod common;
use kaspa_consensus_core::palw_native_resource_v1::{NativeHeapV1, NativeWireScanV1, preflight_native_value_v1};
fn quantities<T: borsh::BorshSerialize + NativeHeapV1 + NativeWireScanV1>(label: &str, value: &T) -> (u64, u64) {
    let wire = borsh::to_vec(value).unwrap();
    let scan = preflight_native_value_v1::<T>(wire.as_slice(), wire.len() as u64).unwrap().logical_requests_v1();
    let heap = value.native_heap_v1(label).bytes().unwrap() + std::mem::size_of::<T>() as u64;
    eprintln!("{label}: wire_request={scan} typed_request={heap}");
    (scan, heap)
}
#[test]
fn golden_history_root_nested_wire_inventory_covers_typed_ownership() {
    use kaspa_consensus_core::palw_tir_court_v1::build_tir_root_claim_v1;
    let (name, program, params, tokens) = common::programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").unwrap();
    let f = common::fixture_with(name, program, params, tokens, 16, 9);
    let x = f.honest();
    let (leaf, _) = f
        .leaves
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_dissect_site_v1(&f.space, f.intervals.as_ref().unwrap(), l)
                .map(|s| (i as u64, s))
        })
        .max_by_key(|(i, s)| (s.history_positions, s.reductions.len(), std::cmp::Reverse(*i)))
        .unwrap();
    let mut root = build_tir_root_claim_v1(&x.binding, leaf, &common::Store { f: &f, x: &x }, &common::RULES).unwrap();
    kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut root.finalize.binding);
    if let kaspa_consensus_core::palw_tir_court_v1::PalwTirParamOpeningV1::Single(proof) = &root.finalize.params {
        let names: usize = proof.opened.iter().map(|(_, op)| op.tensor_name.len()).sum();
        assert!(names > 0);
        eprintln!("artifact names UTF-8 bytes={names}; former one-times String scanner omitted this capacity term");
    } else {
        panic!("the corpus requires actual artifact openings");
    }
    let mut results = Vec::new();
    macro_rules! q {
        ($label:expr,$value:expr) => {
            results.push(($label, quantities($label, $value)));
        };
    }
    q!("elements", &root.elements);
    q!("totals", &root.totals);
    q!("binding", &root.finalize.binding);
    q!("output opening", &root.finalize.output_opening);
    q!("output leaf", &root.finalize.output_preimage);
    q!("operands", &root.finalize.operands);
    q!("params", &root.finalize.params);
    q!("prompt", &root.finalize.prompt_token_ids);
    q!("prompt openings", &root.finalize.prompt_ids_openings);
    q!("decode", &root.finalize.decode_tokens);
    q!("finalize", &root.finalize);
    q!("root", &root);
    let shortages: Vec<_> = results.into_iter().filter(|(_, (scan, heap))| scan < heap).collect();
    assert!(shortages.is_empty(), "nested wire inventory shortages: {shortages:?}");
}

#[test]
fn native_string_wire_inventory_matches_existing_typed_utf8_capacity() {
    for text in ["", "tensor.layer.0", "履歴.attention.α"] {
        let value = text.to_owned();
        let wire = borsh::to_vec(&value).unwrap();
        let proof = preflight_native_value_v1::<String>(wire.as_slice(), wire.len() as u64).unwrap();
        assert_eq!(proof.logical_requests_v1(), (std::mem::size_of::<String>() + 2 * text.len()) as u64);
        assert_eq!(
            proof.logical_requests_v1(),
            value.native_heap_v1("UTF8 string").bytes().unwrap() + std::mem::size_of::<String>() as u64
        );
        assert_eq!(borsh::from_slice::<String>(&wire).unwrap(), value);
        assert!(preflight_native_value_v1::<String>(&wire[..wire.len() - 1], wire.len() as u64 - 1).is_err());
    }
    assert!(preflight_native_value_v1::<String>(&u32::MAX.to_le_bytes()[..], 4).is_err());
}
