//! Local fixture roots are not chain membership or candidate admission evidence.
use kaspa_consensus_core::{
    Hash64, palw_flat_model_seal_v6::*, palw_flat_model_vector_v3::*, palw_kernel_route_v1::*, palw_model_artifact_v2::*,
    palw_model_vector_v2::ModelVectorPostV2, palw_state_v2::PalwBondKeyV2, tx::TransactionOutpoint,
};
use misaka_palw_kernel::{
    descriptor::k2_tir_v4_descriptor,
    evidence::{EvidenceHeaderV1, SuiteParamsV1},
    flat_scope5::flat_resource_scope_v5,
    flat_seal6::FlatSealRowV6,
    plan::plan_for_tir_program_v1,
    public::program_root_v1,
    seg::{SegmentedCommitmentsV1, build_segmented_evidence_v1, prompt_root_of_ids_v1},
    state::ClassRecordV1,
    trace::ParamCommitmentsV1,
};
use misaka_palw_sdk::{
    flat_candidate_v6::{prepare_flat_model_post_v6, snapshot::*},
    onboarding_snapshot::KernelRowsSnapshotV1,
    runtime_pack::commit::hex,
};
use serde_json::{Value, json};

fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}
fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(h(n), n as u32))
}
fn domain() -> Hash64 {
    let p = kaspa_consensus_core::config::drill::palw_chain_params_v1("testnet-12".parse().unwrap(), None).unwrap();
    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash))
}
fn fixture() -> (PalwKernelRouteStateV1, Hash64, Hash64, Hash64) {
    let fx = misaka_palw_tir_sketch::fixture::wide_v1(7);
    let d = k2_tir_v4_descriptor();
    let bytes = fx.program.encode();
    let plan = plan_for_tir_program_v1(&d, &fx.program, program_root_v1(&bytes), 2).unwrap();
    let pc = ParamCommitmentsV1::of(&fx.params);
    let record =
        ClassRecordV1 { descriptor: d.digest(), program_bytes: bytes.clone(), plan: plan.clone(), param_commitments: pc.clone() };
    let mut route = PalwKernelRouteStateV1::new(palw_kernel_route_policy_v1(domain(), h(2)), None, Default::default());
    route.header.scalars.daa = 13;
    let class = h(9);
    route.rows.insert(
        (misaka_palw_kernel::rows::TABLE_CONFORMANCE_CLASSES_V1, borsh::to_vec(&class).unwrap()),
        borsh::to_vec(&([1u8; 64], record)).unwrap(),
    );
    let artifact = ModelArtifactBindingRowV2 {
        descriptor: d.digest(),
        program_root: program_root_v1(&bytes),
        program_bytes_len: bytes.len() as u32,
        model_inventory_root: h(10),
        kernel_param_root: Hash64::from_bytes(pc.root()),
        pc_instances: pc.by_instance.len() as u32,
        max_proof_bytes: 1 << 20,
        binder: bond(1),
        bound_daa: 5,
        matures_daa: 8,
        final_daa: 200,
        reserved: 100,
        refuted: false,
        program_bytes: bytes.clone(),
    };
    let binding = artifact.id();
    route.aux.insert((PALW_MODEL_ARTIFACT_BINDINGS_TABLE_V2, borsh::to_vec(&binding).unwrap()), borsh::to_vec(&artifact).unwrap());
    let prompt = vec![0];
    let trace = misaka_palw_kernel::trace::trace_v1(&fx.program, &fx.params, &prompt).unwrap();
    let c = SegmentedCommitmentsV1::new(
        trace
            .values
            .iter()
            .map(|p| {
                p.iter()
                    .map(|o| o.iter().map(|t| misaka_palw_kernel::flat_court5::flat_profile_commitment_v5(t).unwrap()).collect())
                    .collect()
            })
            .collect(),
    );
    let scope = flat_resource_scope_v5(&fx.program, plan.max_positions, None).unwrap();
    let evidence = build_segmented_evidence_v1(
        EvidenceHeaderV1 {
            network_domain: domain().as_bytes(),
            ruleset_digest: h(2).as_bytes(),
            class_binding_id: class.as_bytes(),
            program_root: plan.program_root,
            artifact_root: pc.root(),
            plan_root: plan.root(),
        },
        &d,
        1,
        &prompt_root_of_ids_v1(&prompt),
        &[],
        &c,
    );
    assert_eq!(evidence.suite, SuiteParamsV1::of(&d));
    let mut post = FlatModelVectorPostV3 {
        version: 3,
        profile: scope.profile,
        scope_root: scope.root(),
        trace: ModelVectorPostV2 {
            version: 2,
            prompt,
            max_new_tokens: 1,
            generated: vec![0],
            evidence,
            segment_roots: c.segment_roots(),
            implementation_roots: [[0; 64]; 3],
        },
    };
    post.trace.implementation_roots = [post.trace_root(class.as_bytes(), binding.as_bytes()); 3];
    let (vector, obj) = prepare_flat_model_post_v6(class, binding, post).unwrap();
    let misaka_palw_kernel::route::KernelRouteObjectV1::PostFlatModelVectorV6 { post, .. } = obj else { panic!() };
    let p = decode_flat_model_vector_post_v6(&post).unwrap();
    let header = FlatModelVectorHeaderV3 {
        trace_root: p.trace_root(class.as_bytes(), binding.as_bytes()),
        class,
        binding,
        profile: scope.profile,
        scope_root: scope.root(),
        program_bytes_len: bytes.len() as u32,
        pc_instances: pc.by_instance.len() as u32,
        post_bytes_len: post.len() as u32,
        max_filing_bytes: scope.worst_court_bytes,
        max_court_work: scope.worst_court_work,
        max_court_scratch_bytes: scope.worst_court_scratch_bytes,
        poster: bond(4),
        posted_daa: 8,
        liability_until: 200,
        reserved: 100,
        refuted: false,
    };
    route.aux.insert(
        (FLAT_MODEL_VECTOR_TABLE_V3, borsh::to_vec(&vector).unwrap()),
        borsh::to_vec(&FlatModelVectorRowV3 { header, post }).unwrap(),
    );
    route.aux.insert((FLAT_PROTECTED_TABLE_V6, borsh::to_vec(&vector).unwrap()), borsh::to_vec(&true).unwrap());
    let seal = FlatNativeSealRowV6 {
        bond: bond(7),
        reserved: 10,
        seal: FlatSealRowV6 {
            version: 6,
            vector: vector.as_bytes(),
            actor: palw_kernel_bond_id_v1(&bond(7)),
            seal: [8; 64],
            posted_daa: 10,
            mature_daa: 13,
            expires_daa: 110,
            spent: false,
        },
    };
    route.aux.insert((FLAT_SEAL_TABLE_V6, borsh::to_vec(&(vector, seal.seal.actor)).unwrap()), borsh::to_vec(&seal).unwrap());
    (route, vector, class, binding)
}
fn anchor(route: &PalwKernelRouteStateV1) -> FlatSnapshotAnchorV6 {
    FlatSnapshotAnchorV6 {
        tip_daa: route.header.scalars.daa,
        network_domain: Hash64::from_bytes(route.header.policy.network_domain),
        ruleset_digest: Hash64::from_bytes(route.header.policy.ruleset_digest),
        ledger_root: route.ledger_root(),
        aux_root: route.aux_root(),
    }
}
fn wire(route: &PalwKernelRouteStateV1, tip: u64, page: kaspa_consensus_core::palw_kernel_route_v1::KernelRowsPageV1) -> Value {
    let (more, next_table, next_key) = match page.next {
        Some((t, k)) => (true, t as u32, hex(&k)),
        None => (false, 0, String::new()),
    };
    json!({"available":true,"tipDaa":tip,"ledgerRoot":route.ledger_root().to_string(),"auxRoot":route.aux_root().to_string(),"header":hex(&borsh::to_vec(&route.header).unwrap()),"rows":page.rows.into_iter().map(|(t,k,r)|json!({"table":t,"key":hex(&k),"row":hex(&r)})).collect::<Vec<_>>(),"more":more,"nextTable":next_table,"nextKey":next_key,"totalRows":page.total_rows})
}
fn pages(route: &PalwKernelRouteStateV1) -> Vec<Value> {
    let mut after = None;
    let mut all = Vec::new();
    loop {
        let page = route.rows_page_v1(after, 512);
        after = page.next.clone();
        all.push(wire(route, route.header.scalars.daa, page));
        if after.is_none() {
            break;
        }
    }
    assert!(all.len() > 1);
    all
}
fn snapshot(pages: &[Value]) -> KernelRowsSnapshotV1 {
    let mut s = KernelRowsSnapshotV1::default();
    for p in pages {
        s.push(flat_rpc_page_v6(&serde_json::to_vec(p).unwrap()).unwrap()).unwrap();
    }
    s
}
#[test]
fn public_pages_anchor_exact_candidate_and_own_mature_seal_without_execution_rights() {
    let (r, v, c, b) = fixture();
    let p = pages(&r);
    let s = FlatPublicSnapshotV6::from_snapshot(snapshot(&p), anchor(&r)).unwrap();
    let (header, view) = s.candidate(v, c, b).unwrap();
    assert_eq!(header.id(), v);
    assert_eq!(view.scope.root(), header.scope_root);
    assert_eq!(s.reveal_row(v, c, b, &bond(7)).unwrap(), r.flat_model_seal_v6(&v, &palw_kernel_bond_id_v1(&bond(7))).unwrap());
    assert_eq!(s.reveal_row(v, c, b, &bond(6)).unwrap_err().code, "NO_FLAT_SEAL");
    assert!(s.candidate(v, h(8), b).is_err());
    assert!(s.candidate(v, c, h(8)).is_err());
    assert!(s.candidate(h(8), c, b).is_err());
    for field in 0..5 {
        let mut a = anchor(&r);
        match field {
            0 => a.tip_daa += 1,
            1 => a.network_domain = h(1),
            2 => a.ruleset_digest = h(1),
            3 => a.ledger_root = h(1),
            _ => a.aux_root = h(1),
        };
        assert_eq!(FlatPublicSnapshotV6::from_snapshot(snapshot(&p), a).err().unwrap().code, "SNAPSHOT_ANCHOR");
    }
}
#[test]
fn rpc_json_rejects_counts_hex_tables_cursors_unknown_fields_and_unavailable_routes() {
    let (r, _, _, _) = fixture();
    let last = pages(&r).pop().unwrap();
    for (field, value, code) in [
        ("available", json!(false), "NO_KERNEL_ROWS"),
        ("totalRows", json!(u64::MAX), "SNAPSHOT_TOO_LARGE"),
        ("header", json!("abx"), "SNAPSHOT_HEX"),
        ("nextTable", json!(1), "SNAPSHOT_CURSOR"),
        ("nextKey", json!("aa"), "SNAPSHOT_CURSOR"),
        ("injected", json!(true), "SNAPSHOT_JSON"),
    ] {
        let mut bad = last.clone();
        bad[field] = value;
        assert_eq!(flat_rpc_page_v6(&serde_json::to_vec(&bad).unwrap()).unwrap_err().code, code);
    }
    let mut bad = last.clone();
    bad["rows"][0]["table"] = json!(256);
    assert_eq!(flat_rpc_page_v6(&serde_json::to_vec(&bad).unwrap()).unwrap_err().code, "SNAPSHOT_TABLE");
    let mut bad = last;
    bad["rows"][0]["key"] = json!("-1");
    assert_eq!(flat_rpc_page_v6(&serde_json::to_vec(&bad).unwrap()).unwrap_err().code, "SNAPSHOT_HEX");
}
#[test]
fn coherent_anchor_does_not_hide_candidate_scope_seal_or_clock_failures() {
    let (r, v, c, b) = fixture();
    for fault in 0..5 {
        let mut changed = r.clone();
        if fault < 3 {
            let key = (FLAT_MODEL_VECTOR_TABLE_V3, borsh::to_vec(&v).unwrap());
            let mut row = changed.flat_model_vector_row_v3(&v).unwrap();
            match fault {
                0 => row.header.max_court_work += 1,
                1 => row.header.refuted = true,
                _ => row.header.liability_until = 13,
            };
            changed.aux.insert(key, borsh::to_vec(&row).unwrap());
        } else {
            let key = (FLAT_SEAL_TABLE_V6, borsh::to_vec(&(v, palw_kernel_bond_id_v1(&bond(7)))).unwrap());
            let mut row = changed.flat_model_seal_v6(&v, &palw_kernel_bond_id_v1(&bond(7))).unwrap();
            if fault == 3 {
                row.seal.spent = true
            } else {
                row.seal.mature_daa = 14
            };
            changed.aux.insert(key, borsh::to_vec(&row).unwrap());
        }
        let s = FlatPublicSnapshotV6::from_snapshot(snapshot(&pages(&changed)), anchor(&changed)).unwrap();
        assert!(s.reveal_row(v, c, b, &bond(7)).is_err());
    }
    let mut bad = pages(&r);
    bad[1]["rows"][0]["row"] = json!("00");
    let s = snapshot(&bad);
    assert_eq!(FlatPublicSnapshotV6::from_snapshot(s, anchor(&r)).err().unwrap().code, "ROWS_NOT_THE_CHAIN");
}
#[test]
fn offline_snapshot_cli_writes_only_exact_row_and_refuses_overwrite_or_wrong_anchor() {
    use std::process::Command;
    let (r, v, c, b) = fixture();
    let dir = std::env::temp_dir().join(format!("flat-snapshot-v6-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let input = dir.join("pages.ndjson");
    let ap = dir.join("anchor.json");
    let out = dir.join("seal.row");
    std::fs::write(&input, pages(&r).iter().map(|p| serde_json::to_string(p).unwrap() + "\n").collect::<String>()).unwrap();
    std::fs::write(&ap, serde_json::to_vec(&anchor(&r)).unwrap()).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_palw-class"))
            .args([
                "flat-v6",
                "snapshot",
                "--network",
                "testnet-12",
                "--bond",
                &format!("{}:{}", bond(7).0.transaction_id, bond(7).0.index),
                "--out",
                out.to_str().unwrap(),
                "--pages",
                input.to_str().unwrap(),
                "--anchor",
                ap.to_str().unwrap(),
                "--vector",
                &v.to_string(),
                "--class",
                &c.to_string(),
                "--binding",
                &b.to_string(),
            ])
            .output()
            .unwrap()
    };
    let p = run();
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    let report: Value = serde_json::from_slice(&p.stdout).unwrap();
    assert_eq!(report["height"], 13);
    assert_eq!(report["trust"], "MATCHED_SUPPLIED_ANCHOR_CHAIN_TRUST_REQUIRED");
    let row = r.flat_model_seal_v6(&v, &palw_kernel_bond_id_v1(&bond(7))).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), borsh::to_vec(&row).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert!(!run().status.success());
    assert_eq!(std::fs::read(&out).unwrap(), borsh::to_vec(&row).unwrap());
    std::fs::remove_file(&out).unwrap();
    let mut a = anchor(&r);
    a.aux_root = h(1);
    std::fs::write(&ap, serde_json::to_vec(&a).unwrap()).unwrap();
    assert!(!run().status.success());
    assert!(!out.exists());
    std::fs::write(&input, serde_json::to_vec(&pages(&r)[0]).unwrap()).unwrap();
    std::fs::write(&ap, serde_json::to_vec(&anchor(&r)).unwrap()).unwrap();
    assert!(!run().status.success());
    assert!(!out.exists());
    std::fs::remove_dir_all(dir).unwrap();
}
