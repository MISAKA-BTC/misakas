//! Bounded storage interpreters retain the stored-format semantics across arbitrary ranges.
//! This exercises decoder APIs; the generic frontend's tensors/blocks binding remains separate.
use misaka_palw_tir_lower::quantfmt::{QuantFormat, QuantRegistry, tensors::RoleTensor};
use serde_json::{Value, json};
use std::cell::Cell;

fn unhex(s: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
    assert!(s.len().is_multiple_of(2));
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16)).collect()
}

fn format(v: Value) -> QuantFormat {
    QuantFormat::from_json(&v.to_string()).expect("valid test descriptor")
}
fn assert_bits(a: &[f32], b: &[f32], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}");
    for (k, (a, b)) in a.iter().zip(b).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "{what} lane {k}");
    }
}
fn tensor_desc() -> Value {
    json!({"schema":"misaka.palw.quant-format.v1", "name":"outsider-ragged-groups",
      "layout":{"kind":"tensors", "roles":[
        {"name":"q", "suffix":".q", "rank":2, "dtypes":["I8"]},
        {"name":"sc", "suffix":".sc", "rank":2, "dtypes":["F32"]},
        {"name":"idx", "suffix":".idx", "rank":1, "dtypes":["I32"], "required":false}],
        "dims":{"out":"dim_q[0]", "inp":"dim_q[1]"},
        "checks":[{"expr":"dim_sc[0] == out && dim_sc[1] == ng", "message":"scale shape"}]},
      "decode":{"target":"integers", "group":{"size":4, "index":"has_idx ? idx[i] : i / gs"},
        "q":"q[o, i] + g", "scale":"sc[o, g] + i", "zero":"o + g + i", "min":"g * 0.25",
        "code":{"min":-128, "max":127}}, "tests":[]})
}
fn tensor_roles() -> Vec<Option<RoleTensor>> {
    vec![
        Some(RoleTensor { shape: vec![3, 10], dtype: "I8".into(), data: (0..30).collect() }),
        Some(RoleTensor {
            shape: vec![3, 3],
            dtype: "F32".into(),
            data: (0..9).flat_map(|i| (i as f32 + 0.5).to_le_bytes()).collect(),
        }),
        Some(RoleTensor {
            shape: vec![10],
            dtype: "I32".into(),
            data: [2i32, 0, 1, 2, 1, 0, 2, 1, 0, 2].into_iter().flat_map(i32::to_le_bytes).collect(),
        }),
    ]
}
fn headers(roles: &[Option<RoleTensor>]) -> Vec<Option<RoleTensor>> {
    roles.iter().map(|r| r.as_ref().map(|r| RoleTensor::header_only(r.shape.clone(), r.dtype.clone()))).collect()
}
fn block_desc() -> Value {
    json!({"schema":"misaka.palw.quant-format.v1", "name":"outsider-global-block-coordinates",
      "layout":{"kind":"blocks", "elems":8, "bytes":12, "fields":[
        {"name":"d", "at":0, "type":"f32"}, {"name":"q", "at":4, "type":"u8", "count":8}]},
      "decode":{"target":"integers", "group":{"size":4},
        "q":"(q[e] + i + row + blk) % 16", "scale":"d + row + blk + j + i",
        "zero":"j + g + row", "min":"d * 0.25", "code":{"min":0,"max":15}}, "tests":[]})
}

#[test]
fn every_builtin_saved_format_matches_its_vectors_and_resident_decoder() {
    let registry = QuantRegistry::builtin();
    let mut counts = (0, 0, 0, 0);
    for f in registry.all() {
        if let Some(b) = f.as_blocks() {
            counts.0 += 1;
            b.streaming_work().unwrap_or_else(|e| panic!("{}: {e}", f.name()));
            for (v, vector) in f.desc.tests.iter().enumerate() {
                let raw = unhex(&vector.block_hex).unwrap();
                let total = raw.len() / b.bytes * b.elems;
                let plan = b.prepare_streamed(1, total).unwrap();
                assert_eq!(plan.bytes(), raw.len());
                let resident = b.decode_floats(&raw, 1, total).unwrap();
                let peak = Cell::new(0);
                let read = |r: std::ops::Range<usize>| {
                    peak.set(peak.get().max(r.len()));
                    assert!(r.len() <= 8);
                    Ok(raw[r].to_vec())
                };
                for step in [1, 7, 31, 1024] {
                    let mut got = Vec::new();
                    for start in (0..total).step_by(step) {
                        got.extend(b.decode_range_streamed(&plan, start..(start + step).min(total), &read).unwrap());
                    }
                    assert_bits(&got, &resident, &format!("{} vector {v} step {step}", f.name()));
                }
                assert!(peak.get() > 0);
                counts.1 += 1;
            }
        } else if let Some(t) = f.as_tensors() {
            counts.2 += 1;
            t.streaming_work().unwrap_or_else(|e| panic!("{}: {e}", f.name()));
            for (v, vector) in f.desc.tests.iter().enumerate() {
                let roles: Vec<_> = t
                    .roles()
                    .map(|(name, _, _)| {
                        vector.roles.get(name).map(|r| RoleTensor {
                            shape: r.shape.clone(),
                            dtype: r.dtype.clone(),
                            data: unhex(&r.hex).unwrap(),
                        })
                    })
                    .collect();
                let cfg = vector.config.clone().unwrap_or(json!({}));
                let params =
                    if f.desc.config.is_some() { t.read_config(&cfg).unwrap().params } else { t.resolve_params(&cfg).unwrap() };
                let (resident, out, inp) = t.decode_floats(&roles, &params).unwrap();
                let plan = t.prepare_streamed(&roles, &params).unwrap();
                assert_eq!(plan.shape(), [out, inp]);
                let headers = headers(&roles);
                let read = |role: usize, r: std::ops::Range<usize>| {
                    assert!(r.len() <= 8);
                    Ok(roles[role].as_ref().unwrap().data[r].to_vec())
                };
                for step in [1, 7, 31, 1024] {
                    let mut got = Vec::new();
                    for start in (0..resident.len()).step_by(step) {
                        got.extend(
                            t.decode_range_streamed(&headers, &plan, start..(start + step).min(resident.len()), &read).unwrap(),
                        );
                    }
                    assert_bits(&got, &resident, &format!("{} vector {v} step {step}", f.name()));
                }
                counts.3 += 1;
            }
        }
    }
    assert_eq!(counts, (31, 186, 9, 40));
}

#[test]
fn tensor_ranges_retain_ragged_reordered_groups_and_the_original_pass_coordinates() {
    let f = format(tensor_desc());
    let t = f.as_tensors().unwrap();
    for indexed in [true, false] {
        let mut roles = tensor_roles();
        if !indexed {
            roles[2] = None;
        }
        let h = headers(&roles);
        let params = t.resolve_params(&json!({})).unwrap();
        let plan = t.prepare_streamed(&h, &params).unwrap();
        let want = t.decode_floats(&roles, &params).unwrap().0;
        for start in 0..30 {
            let got = t
                .decode_range_streamed(&h, &plan, start..(start + 7).min(30), &|role, r| {
                    Ok(roles[role].as_ref().unwrap().data[r].to_vec())
                })
                .unwrap();
            assert_bits(&got, &want[start..(start + 7).min(30)], "ragged group range");
        }
    }
}

#[test]
fn block_ranges_retain_all_global_coordinates_across_rows_blocks_and_groups() {
    let f = format(block_desc());
    let b = f.as_blocks().unwrap();
    let raw: Vec<_> = (0..9).flat_map(|i| (i as f32 + 0.5).to_le_bytes().into_iter().chain(0..8)).collect();
    let plan = b.prepare_streamed(3, 24).unwrap();
    let want = b.decode_floats(&raw, 3, 24).unwrap();
    for start in 0..72 {
        let got = b.decode_range_streamed(&plan, start..(start + 7).min(72), &|r| Ok(raw[r].to_vec())).unwrap();
        assert_bits(&got, &want[start..(start + 7).min(72)], "global block range");
    }
}

#[test]
fn plans_reject_another_descriptor_changed_headers_and_invalid_ranges_before_reads() {
    let f = format(tensor_desc());
    let t = f.as_tensors().unwrap();
    let h = headers(&tensor_roles());
    let plan = t.prepare_streamed(&h, &Default::default()).unwrap();
    let mut other = tensor_desc();
    other["name"] = json!("different");
    let other = format(other);
    let no_read = |_, _| panic!("invalid plan must not read");
    assert!(other.as_tensors().unwrap().decode_range_streamed(&h, &plan, 0..1, &no_read).is_err());
    for roles in [
        h[..2].to_vec(),
        {
            let mut r = h.clone();
            r[0].as_mut().unwrap().shape[1] += 1;
            r
        },
        {
            let mut r = h.clone();
            r[2] = None;
            r
        },
    ] {
        assert!(t.decode_range_streamed(&roles, &plan, 0..1, &no_read).is_err());
    }
    for range in [31..31, 2..1, 0..1025] {
        assert!(t.decode_range_streamed(&h, &plan, range, &no_read).is_err());
    }
    let f = format(block_desc());
    let b = f.as_blocks().unwrap();
    let plan = b.prepare_streamed(1, 2048).unwrap();
    let mut other = block_desc();
    other["name"] = json!("another-block");
    let other = format(other);
    let no_read = |_| panic!("invalid block plan must not read");
    assert!(other.as_blocks().unwrap().decode_range_streamed(&plan, 0..1, &no_read).is_err());
    for range in [2049..2049, 2..1, 0..1025] {
        assert!(b.decode_range_streamed(&plan, range, &no_read).is_err());
    }
}

#[test]
fn header_only_preparation_does_not_allocate_in_proportion_to_tensor_width() {
    let mut d = tensor_desc();
    d["layout"]["checks"] = json!([]);
    let f = format(d);
    let t = f.as_tensors().unwrap();
    let width = 1usize << 40;
    let roles =
        vec![Some(RoleTensor::header_only(vec![3, width], "I8")), Some(RoleTensor::header_only(vec![3, width / 4], "F32")), None];
    let plan = t.prepare_streamed(&roles, &Default::default()).unwrap();
    assert_eq!(plan.shape(), [3, width]);
    let got = t
        .decode_range_streamed(&roles, &plan, width + 3..width + 4, &|role, r| {
            assert!(r.len() <= 4);
            Ok(match role {
                0 => vec![9],
                1 => 0.5f32.to_le_bytes().to_vec(),
                _ => panic!("optional role read"),
            })
        })
        .unwrap();
    assert_bits(&got, &[4.0], "large header range");
}

#[test]
fn metadata_refuses_lane_dependencies_and_requires_resident_role_data() {
    for (section, key, expr) in [("dims", "out", "o + 1"), ("dims", "inp", "q[0, 0] + 1")] {
        let mut d = tensor_desc();
        d["layout"][section][key] = json!(expr);
        let f = format(d);
        assert!(f.as_tensors().unwrap().prepare_streamed(&headers(&tensor_roles()), &Default::default()).is_err());
    }
    let mut d = tensor_desc();
    d["decode"]["code"]["max"] = json!("i + 127");
    assert!(format(d).as_tensors().unwrap().prepare_streamed(&headers(&tensor_roles()), &Default::default()).is_err());
    let mut d = tensor_desc();
    d["layout"]["checks"][0]["expr"] = json!("q[0, 0] == 0");
    assert!(format(d).as_tensors().unwrap().prepare_streamed(&headers(&tensor_roles()), &Default::default()).is_err());
}

#[test]
fn small_shape_metadata_is_bounded_and_snapshotted_once_for_every_range() {
    let d = json!({"schema":"misaka.palw.quant-format.v1", "name":"outsider-shape-document",
      "layout":{"kind":"tensors", "roles":[
        {"name":"q", "suffix":".q", "rank":2, "dtypes":["I8"]},
        {"name":"shape", "suffix":".shape", "rank":1, "dtypes":["I64"]}],
        "dims":{"out":"shape[0]", "inp":"shape[1]"},
        "checks":[{"expr":"dim_q[0] == out && dim_q[1] == inp", "message":"shape mismatch"}]},
      "decode":{"target":"floats", "value":"q[o, i] + shape[0]"}, "tests":[]});
    let f = format(d);
    let t = f.as_tensors().unwrap();
    let roles = vec![
        Some(RoleTensor::header_only(vec![3, 10], "I8")),
        Some(RoleTensor { shape: vec![2], dtype: "I64".into(), data: [3i64, 10].into_iter().flat_map(i64::to_le_bytes).collect() }),
    ];
    let plan = t.prepare_streamed(&roles, &Default::default()).unwrap();
    assert_eq!(plan.metadata_bytes(), 16);
    let got = t
        .decode_range_streamed(&headers(&roles), &plan, 7..14, &|role, r| {
            assert_eq!(role, 0, "metadata must come from the plan snapshot");
            Ok(vec![r.start as u8])
        })
        .unwrap();
    assert_bits(&got, &(10..17).map(|x| x as f32).collect::<Vec<_>>(), "shape metadata snapshot");
    assert!(t.prepare_streamed(&headers(&roles), &Default::default()).is_err());
    let mut oversized = roles.clone();
    oversized[1] = Some(RoleTensor::header_only(vec![8193], "I64"));
    assert!(t.prepare_streamed(&oversized, &Default::default()).is_err());
}

#[test]
fn malformed_sources_and_decoded_values_are_refused() {
    let f = format(tensor_desc());
    let t = f.as_tensors().unwrap();
    let roles = tensor_roles();
    let h = headers(&roles);
    let plan = t.prepare_streamed(&h, &Default::default()).unwrap();
    assert!(t.decode_range_streamed(&h, &plan, 0..1, &|_, _| Ok(vec![])).is_err());
    assert!(
        t.decode_range_streamed(&h, &plan, 0..1, &|role, r| Ok(if role == 2 {
            3i32.to_le_bytes().to_vec()
        } else {
            roles[role].as_ref().unwrap().data[r].to_vec()
        }))
        .is_err()
    );
    let mut d = tensor_desc();
    d["decode"]["code"]["max"] = json!(0);
    let f = format(d);
    let t = f.as_tensors().unwrap();
    let plan = t.prepare_streamed(&h, &Default::default()).unwrap();
    assert!(t.decode_range_streamed(&h, &plan, 1..2, &|role, r| Ok(roles[role].as_ref().unwrap().data[r].to_vec())).is_err());
    let mut d = block_desc();
    d["decode"]["zero"] = json!("32768");
    let f = format(d);
    let b = f.as_blocks().unwrap();
    let plan = b.prepare_streamed(1, 8).unwrap();
    assert!(b.decode_range_streamed(&plan, 0..1, &|r| Ok(vec![0; r.len()])).is_err());
    let mut d = block_desc();
    d["decode"] = json!({"target":"floats","value":"d * 1e38"});
    let f = format(d);
    let b = f.as_blocks().unwrap();
    let plan = b.prepare_streamed(1, 8).unwrap();
    assert!(b.decode_range_streamed(&plan, 0..1, &|_| Ok(10f32.to_le_bytes().to_vec())).is_err());
}

#[test]
fn block_field_and_row_byte_arithmetic_refuse_overflow() {
    for (key, v) in [("at", usize::MAX), ("count", usize::MAX)] {
        let mut d = block_desc();
        d["layout"]["fields"][0][key] = json!(v);
        assert!(QuantFormat::from_json(&d.to_string()).is_err());
    }
    let f = format(block_desc());
    let b = f.as_blocks().unwrap();
    let width = usize::MAX - usize::MAX % 8;
    assert!(b.row_bytes(width).is_err());
    assert!(b.prepare_streamed(usize::MAX, 8).is_err());
    assert!(b.prepare_streamed(0, 8).is_err());
    assert!(b.prepare_streamed(1, 7).is_err());
}
