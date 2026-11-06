#![cfg(feature = "node")]
//! Hello2 over a pre-admitted resident toy plan. This is not generic artifact
//! startup, native admission, GPU support, or computation-credit certification.
mod run_wide_common;

use kaspa_consensus_core::{
    Hash64,
    palw_tir_step_v1::{PalwTirStepBindingV1, PalwTirStepSpaceV2, palw_tir_execution_root_v1, verify_tir_binding_v2},
};
use misaka_palw_tir::DType;
use misaka_palw_tir_exec::{
    NodeValue, StepSink,
    cellproc::{CellRequestV1, CellResponseV1},
    node::cell_ipc_v2::{
        CellParamBlobV1, CellRequestWideV2 as Request, CellResponseWideV2 as Response, CellValueV1, PreparedWideCellV2,
        WideCellClientV2, WideCellLimitsV2, codec, quote_client, quote_server, serve_wide_v2_for_plan,
    },
};
use run_wide_common::*;
use std::{
    io::{Cursor, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const EXTERNAL: u64 = 64 << 20; // Conservative declared owner inventory, not measured RSS.
const SCRATCH: u64 = 8192;

fn limits() -> WideCellLimitsV2 {
    WideCellLimitsV2 {
        max_frame_bytes: 4 << 20,
        max_decoded_bytes: 4 << 20,
        max_operation_bytes: 1 << 30,
        max_total_read_bytes: 64 << 20,
        max_total_write_bytes: 64 << 20,
        max_messages: 1024,
        max_steps: 64,
        max_program_bytes: 1 << 20,
        max_params: 16,
        max_param_bytes: 1 << 20,
        max_values: 128,
        max_lanes: 1 << 16,
        max_carry_vectors: 64,
        max_rank: 8,
        max_name_bytes: 1024,
        max_workspace_bytes: 512 << 20,
        max_work: 1 << 48,
        external_retained_bytes: EXTERNAL,
    }
}

fn prepared_binding(f: &Fixture<'_>) -> PalwTirStepBindingV1 {
    let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
    let trace = Hash64::from_u64_word(61);
    let step = Hash64::from_u64_word(62);
    let count = space.leaf_count_capped(&f.job, LEAF_CAP).unwrap();
    PalwTirStepBindingV1 {
        version: 1,
        job_context: f.job.clone(),
        class: f.class.clone(),
        artifact_root: f.artifact,
        full_logits_trace_root: trace,
        step_leaf_count: count,
        step_merkle_root: step,
        committed_execution_root: palw_tir_execution_root_v1(&f.job.context_hash(), &trace, &f.job.shape_profile_id, count, &step),
    }
}

fn blobs(f: &Fixture<'_>, post: bool) -> Vec<CellParamBlobV1> {
    (0..if post { 2 } else { 1 })
        .map(|param| {
            let declaration = &f.program.params[param];
            let bytes = f
                .params
                .get(param as u16, None)
                .unwrap()
                .to_i128s()
                .into_iter()
                .flat_map(|value| value.to_le_bytes()[..declaration.dtype.width()].to_vec())
                .collect();
            CellParamBlobV1 { param: param as u16, layer: None, bytes }
        })
        .collect()
}

fn open(f: &Fixture<'_>) -> Request {
    Request::Open { program: f.program.encode(), occ: (0, 2), params: blobs(f, true) }
}

fn framed(body: &[u8]) -> Vec<u8> {
    let mut bytes = (body.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(body);
    bytes
}

fn response_frames(responses: &[Response]) -> Vec<u8> {
    responses.iter().flat_map(|response| framed(&response.encode_bounded(limits()).unwrap())).collect()
}

fn ready() -> Response {
    Response::Ready { version: 2, name: "independent-test-peer".into(), capacity: Some(12345) }
}

#[derive(Clone)]
struct AuditWriter(Arc<Mutex<Vec<u8>>>);
impl Write for AuditWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn scripted(responses: &[Response], cap: WideCellLimitsV2) -> (WideCellClientV2<Cursor<Vec<u8>>, AuditWriter>, Arc<Mutex<Vec<u8>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = WideCellClientV2::connect(Cursor::new(response_frames(responses)), AuditWriter(sent.clone()), cap).unwrap();
    (client, sent)
}

#[derive(Default)]
struct Collect(Vec<CellValueV1>);
impl StepSink for Collect {
    fn node(&mut self, value: &NodeValue<'_>) {
        assert!(value.commit);
        self.0.push(CellValueV1 {
            slot: value.slot,
            block: value.block,
            layer: value.layer,
            node: value.node,
            dtype: value.dtype.tag(),
            shape: value.shape.iter().map(|dim| *dim as u32).collect(),
            lanes: value.data.to_i128s(),
        });
    }
}

fn expected_values(f: &Fixture<'_>, position: u32) -> Vec<CellValueV1> {
    f.nodes
        .iter()
        .filter(|node| node.position == position && node.commit)
        .map(|node| CellValueV1 {
            slot: node.slot,
            block: node.block,
            layer: node.layer,
            node: node.node,
            dtype: node.value.dtype.tag(),
            shape: node.value.shape.iter().map(|dim| *dim as u32).collect(),
            lanes: node.value.data.clone(),
        })
        .collect()
}

fn row_bytes(position: usize) -> Vec<u8> {
    let token = EXPECTED_TOKENS[position] as usize;
    EMBEDDING[token * 4..token * 4 + 4].iter().flat_map(|value| i32::try_from(*value).unwrap().to_le_bytes()).collect()
}

#[test]
fn hello2_wire_has_literal_i64_logits_and_preserves_all_other_v1_message_bytes() {
    assert_eq!(codec::CELL_WIRE_VERSION_V2, 2);
    let requests = [
        Request::Hello { version: 2 },
        Request::Open { program: vec![7, 8], occ: (0, 2), params: vec![CellParamBlobV1 { param: 3, layer: Some(4), bytes: vec![9] }] },
        Request::Step { token: 3, occ: (0, 1), carry_in: vec![vec![i128::MIN, 17], vec![]] },
        Request::Fixed { state: 2, layer: None, first: 3, n: 4 },
        Request::Hist { state: 5, layer: Some(6), h_tile: 2, first_lane: 1, row_lanes: 4 },
        Request::Close,
    ];
    for request in requests {
        let body = request.encode_bounded(limits()).unwrap();
        assert_eq!(Request::decode_bounded(&body, limits()).unwrap(), request);
        let old = CellRequestV1::decode(&body).unwrap();
        assert_eq!(old.encode(), body);
        assert_eq!(codec::encoded_length_request(&request).unwrap(), body.len() as u64);
    }
    let wide = Response::Stepped { values: vec![], carry: vec![], logits: vec![i64::MIN, 1082i64 << 24, i64::MAX] };
    let mut literal = vec![2];
    literal.extend_from_slice(&0u32.to_le_bytes());
    literal.extend_from_slice(&0u32.to_le_bytes());
    literal.extend_from_slice(&3u64.to_le_bytes());
    for value in [i64::MIN, 1082i64 << 24, i64::MAX] {
        literal.extend_from_slice(&value.to_le_bytes());
    }
    assert_eq!(wide.encode_bounded(limits()).unwrap(), literal);
    assert_eq!(Response::decode_bounded(&literal, limits()).unwrap(), wide);
    assert!(CellResponseV1::decode(&literal).is_err());
    let old = CellResponseV1::Stepped { values: vec![], carry: vec![], logits: vec![i32::MIN, 17, i32::MAX] };
    let old_body = old.encode();
    assert_eq!(old_body.len(), 1 + 4 + 4 + 8 + 3 * 4);
    assert_eq!(CellResponseV1::decode(&old_body).unwrap(), old);
    assert!(Response::decode_bounded(&old_body, limits()).is_err());
    for response in
        [ready(), Response::Opened, Response::Lanes(vec![255, 255, 255, 255]), Response::Closed, Response::Refused("why".into())]
    {
        let body = response.encode_bounded(limits()).unwrap();
        assert_eq!(Response::decode_bounded(&body, limits()).unwrap(), response);
        assert_eq!(CellResponseV1::decode(&body).unwrap().encode(), body);
        assert_eq!(codec::encoded_length_response(&response).unwrap(), body.len() as u64);
    }
}

#[test]
fn codec_caps_native_headers_aggregate_lanes_truncation_and_owned_spare_capacity() {
    let mut request = Request::Step { token: 0, occ: (0, 1), carry_in: vec![vec![1, -2, i128::MAX], vec![]] };
    let original = codec::native_backing_request(&request).unwrap();
    if let Request::Step { carry_in, .. } = &mut request {
        carry_in.reserve_exact(7);
        carry_in[0].reserve_exact(13);
    }
    let native = codec::native_backing_request(&request).unwrap();
    assert!(native > original);
    let frame = codec::encoded_length_request(&request).unwrap();
    let mut exact = limits();
    exact.max_frame_bytes = frame;
    exact.max_decoded_bytes = native;
    exact.max_operation_bytes = frame + native + SCRATCH + EXTERNAL;
    let body = request.encode_bounded(exact).unwrap();
    for field in 0..5 {
        let mut cap = exact;
        match field {
            0 => cap.max_frame_bytes -= 1,
            1 => cap.max_decoded_bytes -= 1,
            2 => cap.max_operation_bytes -= 1,
            3 => cap.max_lanes = 2,
            _ => cap.max_carry_vectors = 1,
        }
        assert!(request.encode_bounded(cap).is_err(), "request cap {field}");
    }
    for n in 0..body.len() {
        assert!(Request::decode_bounded(&body[..n], limits()).is_err());
    }
    let mut trailing = body;
    trailing.push(0);
    assert!(Request::decode_bounded(&trailing, limits()).is_err());
    let values = vec![CellValueV1 {
        slot: 5,
        block: 1,
        layer: None,
        node: 1,
        dtype: DType::I64.tag(),
        shape: vec![2],
        lanes: vec![i64::MIN as i128, i64::MAX as i128],
    }];
    let response = Response::Stepped { values, carry: vec![vec![9]], logits: vec![i64::MIN, i64::MAX] };
    let body = response.encode_bounded(limits()).unwrap();
    let decoded = Response::decode_bounded(&body, limits()).unwrap();
    assert_eq!(decoded, response);
    let native = codec::native_backing_response(&decoded).unwrap();
    let mut exact = limits();
    exact.max_frame_bytes = body.len() as u64;
    exact.max_decoded_bytes = native;
    exact.max_operation_bytes = body.len() as u64 + native + SCRATCH + EXTERNAL;
    assert!(Response::decode_bounded(&body, exact).is_ok());
    for field in 0..6 {
        let mut cap = exact;
        match field {
            0 => cap.max_decoded_bytes -= 1,
            1 => cap.max_operation_bytes -= 1,
            2 => cap.max_values = 0,
            3 => cap.max_lanes = 4, // 2 value + 1 carry + 2 logits is five.
            4 => cap.max_rank = 0,
            _ => cap.max_carry_vectors = 0,
        }
        assert!(Response::decode_bounded(&body, cap).is_err(), "response cap {field}");
    }
    let invalid = CellResponseV1::Stepped {
        values: vec![CellValueV1 {
            slot: 0,
            block: 0,
            layer: None,
            node: 0,
            dtype: DType::I8.tag(),
            shape: vec![1],
            lanes: vec![128],
        }],
        carry: vec![],
        logits: vec![],
    }
    .encode();
    assert!(Response::decode_bounded(&invalid, limits()).is_err());
    let invalid = Response::Stepped {
        values: vec![CellValueV1 {
            slot: 0,
            block: 0,
            layer: None,
            node: 0,
            dtype: DType::I8.tag(),
            shape: vec![1],
            lanes: vec![128],
        }],
        carry: vec![],
        logits: vec![],
    };
    assert!(codec::encoded_length_response(&invalid).is_ok()); // Header-only count, not a lane scan.
    assert!(invalid.encode_bounded(limits()).is_err());
    let mut empty_carry = vec![2];
    empty_carry.extend_from_slice(&[0; 12]); // token + occurrence range
    empty_carry.extend_from_slice(&4096u32.to_le_bytes());
    empty_carry.resize(empty_carry.len() + 4096 * 8, 0);
    let mut cap = limits();
    cap.max_carry_vectors = 4096;
    cap.max_decoded_bytes = 64 << 10;
    assert!(4096 * std::mem::size_of::<Vec<i128>>() as u64 > cap.max_decoded_bytes);
    assert!(Request::decode_bounded(&empty_carry, cap).unwrap_err().contains("decoded storage"));
    for offset in [13usize, 17] {
        // outer carry u32, first lane count u64
        let mut hostile = request.encode_bounded(limits()).unwrap();
        if offset == 13 {
            hostile[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        } else {
            hostile[offset..offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        }
        assert!(Request::decode_bounded(&hostile, limits()).is_err());
    }
}

#[test]
fn pre_admitted_cpu_keeps_exact_wide_rows_old_state_bytes_and_workspace_guard_scope() {
    use misaka_palw_tir_exec::node::memory::set_tir_memory_reserver_v1;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    let target_thread = std::thread::current().id();
    let held = Arc::new(AtomicU64::new(0));
    let opens = Arc::new(AtomicUsize::new(0));
    let refuse = Arc::new(AtomicBool::new(false));
    struct Lease(Arc<AtomicU64>, u64);
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.fetch_sub(self.1, Ordering::SeqCst);
        }
    }
    assert!(set_tir_memory_reserver_v1({
        let (held, opens, refuse) = (held.clone(), opens.clone(), refuse.clone());
        Arc::new(move |role, _, _, bytes| {
            if role != "tir-wide-cell-v2" || std::thread::current().id() != target_thread {
                return Ok(Box::new(()));
            }
            if refuse.load(Ordering::SeqCst) {
                return Err("independent workspace refusal".into());
            }
            opens.fetch_add(1, Ordering::SeqCst);
            held.fetch_add(bytes, Ordering::SeqCst);
            Ok(Box::new(Lease(held.clone(), bytes)))
        })
    }));
    with_fixture(3, |wide, narrow| {
        let binding = prepared_binding(wide);
        let verified = verify_tir_binding_v2(&binding, LEAF_CAP).unwrap();
        let prepared = PreparedWideCellV2::new(&verified, &wide.job, wide.plan, wide.params, limits()).unwrap();
        let quote = quote_server(verified.space(), wide.plan, &wide.job, limits()).unwrap();
        assert_eq!(
            quote.operation_peak_bytes,
            2 * limits().max_frame_bytes + 2 * limits().max_decoded_bytes + quote.workspace_bytes + SCRATCH + EXTERNAL
        );
        assert!(PreparedWideCellV2::new(&verified, &wide.job, narrow.plan, narrow.params, limits()).is_err());
        prepared.check_open(&wide.program.encode(), 0..2, &blobs(wide, true), limits()).unwrap();
        for attack in 0..4 {
            let mut params = blobs(wide, true);
            match attack {
                0 => params[0].bytes[0] ^= 1,
                1 => {
                    params.pop();
                }
                2 => params.push(params[0].clone()),
                _ => params[0].layer = Some(0),
            }
            assert!(prepared.check_open(&wide.program.encode(), 0..2, &params, limits()).is_err());
        }
        assert!(prepared.open_cell(1..2, limits()).is_err()); // No implicit prefill resume for a post-only cell.
        refuse.store(true, Ordering::SeqCst);
        assert!(prepared.open_cell(0..2, limits()).is_err());
        assert_eq!(held.load(Ordering::SeqCst), 0);
        refuse.store(false, Ordering::SeqCst);
        let mut cell = prepared.open_cell(0..2, limits()).unwrap();
        assert_eq!(held.load(Ordering::SeqCst), prepared.workspace_bytes());
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        let mut sink = Collect::default();
        assert!(cell.step_data(0, 0..2, &[], &mut sink).is_err()); // First prefill skips post.
        assert!(cell.step_data(4, 0..1, &[], &mut sink).is_err());
        assert!(cell.step_data(0, 0..1, &[vec![0]], &mut sink).is_err());
        assert!(sink.0.is_empty());
        assert_eq!(cell.position(), 0);
        for (position, token) in EXPECTED_TOKENS.iter().copied().enumerate() {
            let range = if position == 0 { 0..1 } else { 0..2 };
            assert_eq!(cell.expected_range().unwrap(), range);
            assert!(cell.kernel_work_for_next(range.clone()).unwrap() > 0);
            let mut sink = Collect::default();
            let carry = cell.step_data(token, range, &[], &mut sink).unwrap();
            assert_eq!(sink.0, expected_values(wide, position as u32));
            if position == 0 {
                assert_eq!(carry, vec![EMBEDDING[0..4].to_vec()]);
                assert!(cell.logits_lanes().unwrap().is_empty());
            } else {
                assert!(carry.is_empty());
                assert_eq!(cell.logits_lanes().unwrap(), wide.rows[position - 1]);
            }
            assert_eq!(cell.fixed_lanes(0, None, 0, 4).unwrap(), row_bytes(position));
            if position == 0 {
                assert!(cell.hist_tile_lanes(1, None, 2, 0, 4).is_err());
            } else {
                assert_eq!(cell.hist_tile_lanes(1, None, 2, 0, 4).unwrap(), [row_bytes(position - 1), row_bytes(position)].concat());
            }
            assert!(cell.fixed_lanes(1, None, 0, 4).is_err());
            assert!(cell.fixed_lanes(0, Some(0), 0, 4).is_err());
            assert!(cell.fixed_lanes(0, None, 0, 5).is_err());
        }
        assert!(cell.expected_range().is_err());
        assert_eq!(held.load(Ordering::SeqCst), prepared.workspace_bytes());
        drop(cell);
        assert_eq!(held.load(Ordering::SeqCst), 0);
        prepared.check_open(&wide.program.encode(), 0..1, &blobs(wide, false), limits()).unwrap();
        assert!(prepared.check_open(&wide.program.encode(), 0..1, &blobs(wide, true), limits()).is_err());
        let mut pre_only = prepared.open_cell(0..1, limits()).unwrap();
        let mut sink = Collect::default();
        assert_eq!(pre_only.step_data(1, 0..1, &[], &mut sink).unwrap(), vec![EMBEDDING[4..8].to_vec()]);
        assert!(pre_only.logits_lanes().unwrap().is_empty());
        drop(pre_only);
        assert_eq!(held.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn client_poisoning_prefix_charges_and_exports_survive_close_without_v1_fallback() {
    with_fixture(3, |wide, _| {
        for misuse in [Request::Hello { version: 2 }, Request::Step { token: 0, occ: (0, 1), carry_in: vec![] }] {
            let (mut client, sent) = scripted(&[ready()], limits());
            assert_eq!(client.peer_name(), "independent-test-peer");
            assert_eq!(client.peer_capacity(), Some(12345));
            let before = client.usage();
            assert_eq!(before.messages, 2);
            assert_eq!(before.written_bytes, 8 + 5);
            assert_eq!(before.read_bytes, 8 + codec::encoded_length_response(&ready()).unwrap());
            let wire = sent.lock().unwrap().clone();
            let error = client.request(&misuse).unwrap_err();
            assert!(client.poisoned());
            assert_eq!(error.usage, client.usage());
            assert_eq!(client.usage(), before);
            assert_eq!(*sent.lock().unwrap(), wire);
            assert!(client.request(&open(wide)).is_err());
            assert_eq!(client.usage(), before);
        }
        for response in [Response::Closed, Response::Refused("independent rejection".into())] {
            let (mut client, _) = scripted(&[ready(), response], limits());
            let before = client.usage();
            let error = client.request(&open(wide)).unwrap_err();
            assert!(client.poisoned());
            assert_eq!(error.usage, client.usage());
            assert!(error.usage.read_bytes > before.read_bytes && error.usage.written_bytes > before.written_bytes);
            assert_eq!(error.usage.exported_bytes, before.exported_bytes);
        }

        let sent = Arc::new(Mutex::new(Vec::new()));
        let mut wire = response_frames(&[ready()]);
        wire.extend_from_slice(&(limits().max_frame_bytes + 1).to_le_bytes()); // No hostile body is sent or allocated.
        let mut client = WideCellClientV2::connect(Cursor::new(wire), AuditWriter(sent), limits()).unwrap();
        let before = client.usage();
        let error = client.request(&open(wide)).unwrap_err();
        assert!(client.poisoned());
        assert_eq!(error.usage.read_bytes, before.read_bytes); // Rejected prefix is not accepted logical I/O.
        assert_eq!(error.usage.written_bytes, before.written_bytes + codec::encoded_length_request(&open(wide)).unwrap() + 8);
        assert_eq!(error.usage.messages, before.messages + 1);

        let replies = [ready(), Response::Opened, Response::Closed, Response::Opened];
        let (mut probe, _) = scripted(&replies, limits());
        probe.request(&open(wide)).unwrap();
        let after_open = probe.usage();
        probe.request(&Request::Close).unwrap();
        let after_close = probe.usage();
        assert_eq!(after_close.messages, 6);
        assert!(after_close.exported_bytes > after_open.exported_bytes);
        assert_eq!(after_close.exported_bytes - after_open.exported_bytes, codec::native_backing_response(&Response::Closed).unwrap());
        for exact in [true, false] {
            let mut cap = limits();
            cap.max_work = after_close.work - u64::from(!exact);
            let (mut capped, _) = scripted(&replies, cap);
            capped.request(&open(wide)).unwrap();
            let before = capped.usage();
            let result = capped.request(&Request::Close);
            if exact {
                assert_eq!(result.unwrap(), Response::Closed);
                assert_eq!(capped.usage().work, after_close.work);
            } else {
                assert!(result.unwrap_err().message.contains("work"));
                assert!(capped.poisoned());
                assert!(capped.usage().work >= before.work);
                assert!(capped.usage().work <= cap.max_work);
                assert_eq!(capped.usage().exported_bytes, before.exported_bytes);
            }
        }
        for query in [
            Request::Fixed { state: 0, layer: None, first: 0, n: 1 },
            Request::Hist { state: 1, layer: None, h_tile: 1, first_lane: 0, row_lanes: 1 },
        ] {
            for bytes in [vec![], vec![0; 4], vec![0; 8]] {
                let (mut client, _) = scripted(&[ready(), Response::Opened, Response::Lanes(bytes.clone())], limits());
                client.request(&open(wide)).unwrap();
                let before = client.usage();
                let result = client.request(&query);
                if bytes.len() == 4 {
                    assert_eq!(result.unwrap(), Response::Lanes(bytes));
                    assert!(!client.poisoned());
                } else {
                    assert!(result.unwrap_err().message.contains("query response length"));
                    assert!(client.poisoned());
                    assert_eq!(client.usage().exported_bytes, before.exported_bytes);
                }
            }
        }
        let mut cap = limits();
        cap.max_messages = after_close.messages;
        let (mut capped, sent) = scripted(&replies, cap);
        capped.request(&open(wide)).unwrap();
        capped.request(&Request::Close).unwrap();
        let before = capped.usage();
        let written = sent.lock().unwrap().len();
        assert!(capped.request(&open(wide)).is_err());
        assert!(capped.poisoned());
        assert_eq!(capped.usage().messages, before.messages);
        assert_eq!(capped.usage().read_bytes, before.read_bytes);
        assert_eq!(capped.usage().written_bytes, before.written_bytes);
        assert_eq!(capped.usage().exported_bytes, before.exported_bytes);
        assert!(capped.usage().work >= before.work);
        assert_eq!(sent.lock().unwrap().len(), written);

        // Open and Close fit; the next operation must reserve every earlier
        // returned response even though protocol state has returned to Ready.
        let base = quote_client(limits()).unwrap();
        let mut cap = limits();
        cap.max_operation_bytes = base.operation_peak_bytes + after_open.exported_bytes;
        let (mut capped, sent) = scripted(&replies, cap);
        capped.request(&open(wide)).unwrap();
        capped.request(&Request::Close).unwrap();
        let before = capped.usage();
        let written = sent.lock().unwrap().len();
        assert!(capped.request(&open(wide)).unwrap_err().message.contains("retained export"));
        assert_eq!(capped.usage(), before);
        assert_eq!(sent.lock().unwrap().len(), written);
        assert!(before.exported_bytes > after_open.exported_bytes);

        for read_side in [false, true] {
            let mut cap = limits();
            if read_side {
                cap.max_total_read_bytes = after_close.read_bytes - 1;
            } else {
                cap.max_total_write_bytes = after_close.written_bytes - 1;
            }
            let (mut client, _) = scripted(&replies, cap);
            client.request(&open(wide)).unwrap();
            let before = client.usage();
            let error = client.request(&Request::Close).unwrap_err();
            assert!(client.poisoned());
            assert_eq!(error.usage, client.usage());
            assert!(error.usage.read_bytes >= before.read_bytes && error.usage.written_bytes >= before.written_bytes);
            assert!(error.message.contains("cumulative"));
        }
        let sent = Arc::new(Mutex::new(Vec::new()));
        let wrong_ready = Response::Ready { version: 1, name: "old-peer".into(), capacity: None };
        let refusal = WideCellClientV2::connect(Cursor::new(response_frames(&[wrong_ready])), AuditWriter(sent.clone()), limits())
            .err()
            .unwrap();
        assert!(refusal.message.contains("no V1 fallback"));
        assert_eq!(*sent.lock().unwrap(), framed(&Request::Hello { version: 2 }.encode_bounded(limits()).unwrap()));
    });
}

fn decode_replies(bytes: &[u8]) -> Vec<Response> {
    let mut cursor = Cursor::new(bytes);
    let mut responses = Vec::new();
    while cursor.position() < bytes.len() as u64 {
        let mut prefix = [0; 8];
        cursor.read_exact(&mut prefix).unwrap();
        let length = usize::try_from(u64::from_le_bytes(prefix)).unwrap();
        assert!(length <= bytes.len());
        let mut body = vec![0; length];
        cursor.read_exact(&mut body).unwrap();
        responses.push(Response::decode_bounded(&body, limits()).unwrap());
    }
    responses
}

#[test]
fn server_requires_hello2_and_never_resets_step_or_io_quotas_on_reopen() {
    with_fixture(3, |wide, _| {
        let binding = prepared_binding(wide);
        let verified = verify_tir_binding_v2(&binding, LEAF_CAP).unwrap();
        let prepared = PreparedWideCellV2::new(&verified, &wide.job, wide.plan, wide.params, limits()).unwrap();
        for requests in
            [vec![open(wide)], vec![Request::Hello { version: 1 }], vec![Request::Hello { version: 2 }, Request::Hello { version: 2 }]]
        {
            let wire: Vec<_> = requests.iter().flat_map(|request| framed(&request.encode_bounded(limits()).unwrap())).collect();
            let mut output = Vec::new();
            let error = serve_wide_v2_for_plan(&mut Cursor::new(&wire), &mut output, &prepared, limits()).unwrap_err();
            assert_eq!(error.usage.steps, 0);
            assert_eq!(error.usage.read_bytes, wire.len() as u64);
            let replies = decode_replies(&output);
            assert!(matches!(replies.last(), Some(Response::Refused(_))));
            assert!(!replies.iter().any(|response| matches!(response, Response::Opened | Response::Stepped { .. })));
            assert_eq!(error.usage.written_bytes, output.len() as u64);
        }
        let requests = [
            Request::Hello { version: 2 },
            open(wide),
            Request::Step { token: 0, occ: (0, 1), carry_in: vec![] },
            Request::Close,
            open(wide),
            Request::Step { token: 0, occ: (0, 1), carry_in: vec![] },
        ];
        let wire: Vec<_> = requests.iter().flat_map(|request| framed(&request.encode_bounded(limits()).unwrap())).collect();
        let mut cap = limits();
        cap.max_steps = 1;
        let mut output = Vec::new();
        let error = serve_wide_v2_for_plan(&mut Cursor::new(&wire), &mut output, &prepared, cap).unwrap_err();
        assert!(error.message.contains("cumulative step"));
        assert_eq!(error.usage.steps, 1);
        assert_eq!(error.usage.read_bytes, wire.len() as u64);
        assert_eq!(error.usage.written_bytes, output.len() as u64);
        let replies = decode_replies(&output);
        assert_eq!(replies.iter().filter(|reply| matches!(reply, Response::Opened)).count(), 2);
        assert_eq!(replies.iter().filter(|reply| matches!(reply, Response::Closed)).count(), 1);
        assert_eq!(replies.iter().filter(|reply| matches!(reply, Response::Stepped { .. })).count(), 1);
        assert!(matches!(replies.last(), Some(Response::Refused(_))));
        assert_eq!(error.usage.messages, requests.len() as u64 + replies.len() as u64);
        assert!(error.usage.work >= 128 * (wire.len() as u64 + output.len() as u64));
    });
}

struct ChildGuard(Option<Child>);
impl ChildGuard {
    fn finish(mut self) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = self.0.as_mut().unwrap().try_wait().unwrap() {
                self.0.take();
                assert!(status.success(), "toy IPC child exited {status}");
                return;
            }
            assert!(Instant::now() < deadline, "toy IPC child did not exit after stream closure");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn child_connection(mode: &str) -> (ChildGuard, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "hello2_process_child", "--ignored", "--nocapture"])
        .env("MISAKA_HELLO2_TEST_ADDR", listener.local_addr().unwrap().to_string())
        .env("MISAKA_HELLO2_TEST_MODE", mode)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let guard = ChildGuard(Some(child));
    let deadline = Instant::now() + Duration::from_secs(15);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "toy IPC child never connected");
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("toy IPC accept failed: {error}"),
        }
    };
    // Darwin may preserve the listener's nonblocking flag on an accepted socket.
    // The protocol uses blocking reads with finite timeouts, independent of the
    // platform's accept flag inheritance.
    stream.set_nonblocking(false).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    stream.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
    (guard, stream)
}

#[test]
fn actual_toy_process_hello2_preserves_wide_values_and_rejects_both_v1_directions() {
    with_fixture(3, |wide, _| {
        let (child, stream) = child_connection("wide");
        let mut client = WideCellClientV2::connect(stream.try_clone().unwrap(), stream, limits()).unwrap();
        let initial = client.usage();
        let (mut read, mut written) = (initial.read_bytes, initial.written_bytes);
        let mut exchange = |client: &mut WideCellClientV2<TcpStream, TcpStream>, request: Request| {
            written += codec::encoded_length_request(&request).unwrap() + 8;
            let response = client.request(&request).unwrap();
            read += codec::encoded_length_response(&response).unwrap() + 8;
            assert_eq!(client.usage().read_bytes, read);
            assert_eq!(client.usage().written_bytes, written);
            response
        };
        assert_eq!(exchange(&mut client, open(wide)), Response::Opened);
        for (position, token) in EXPECTED_TOKENS.iter().copied().enumerate() {
            let response =
                exchange(&mut client, Request::Step { token, occ: if position == 0 { (0, 1) } else { (0, 2) }, carry_in: vec![] });
            let Response::Stepped { values, carry, logits } = response else { panic!("expected exact wide step data") };
            assert_eq!(values, expected_values(wide, position as u32));
            if position == 0 {
                assert_eq!(carry, vec![EMBEDDING[0..4].to_vec()]);
                assert!(logits.is_empty());
            } else {
                assert!(carry.is_empty());
                assert_eq!(logits, wide.rows[position - 1]);
            }
            assert_eq!(
                exchange(&mut client, Request::Fixed { state: 0, layer: None, first: 0, n: 4 }),
                Response::Lanes(row_bytes(position))
            );
            if position > 0 {
                assert_eq!(
                    exchange(&mut client, Request::Hist { state: 1, layer: None, h_tile: 2, first_lane: 0, row_lanes: 4 }),
                    Response::Lanes([row_bytes(position - 1), row_bytes(position)].concat())
                );
            }
        }
        assert_eq!(exchange(&mut client, Request::Close), Response::Closed);
        assert_eq!(client.usage().steps, 7);
        assert!(!client.poisoned());
        drop(client);
        child.finish();
    });
    let (child, stream) = child_connection("legacy");
    let refusal = WideCellClientV2::connect(stream.try_clone().unwrap(), stream, limits()).err().unwrap();
    assert!(refusal.message.contains("version") || refusal.message.contains("wire"));
    child.finish();
    let (child, stream) = child_connection("wide-reject");
    assert!(
        misaka_palw_tir_exec::TirProcessDeviceV1::connect(Box::new(stream.try_clone().unwrap()), Box::new(stream), false).is_err()
    );
    child.finish();
}

#[test]
#[ignore = "helper invoked only by the bounded loopback subprocess integration"]
fn hello2_process_child() {
    let Ok(address) = std::env::var("MISAKA_HELLO2_TEST_ADDR") else { return };
    let mode = std::env::var("MISAKA_HELLO2_TEST_MODE").unwrap();
    let stream = TcpStream::connect(address).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    stream.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
    let (mut reader, mut writer) = (stream.try_clone().unwrap(), stream);
    if mode == "legacy" {
        misaka_palw_tir_exec::serve_cell_requests_v1(&mut reader, &mut writer, &misaka_palw_tir_exec::CpuDeviceV1).unwrap();
        return;
    }
    with_fixture(3, |wide, _| {
        // Child trusts only this independently prepared local toy declaration,
        // plan/params and consistent binding. Roots here are synthetic data
        // identities; neither a prior run nor native credit is asserted.
        let binding = prepared_binding(wide);
        let verified = verify_tir_binding_v2(&binding, LEAF_CAP).unwrap();
        let prepared = PreparedWideCellV2::new(&verified, &wide.job, wide.plan, wide.params, limits()).unwrap();
        let result = serve_wide_v2_for_plan(&mut reader, &mut writer, &prepared, limits());
        if mode == "wide-reject" {
            assert!(result.is_err());
        } else {
            let usage = result.unwrap();
            assert_eq!(usage.steps, 7);
        }
    });
}
