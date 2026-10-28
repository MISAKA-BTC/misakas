use super::*;
use kaspa_consensus_core::{
    palw_qwen36_profile::{QWEN36_35B_A3B, qwen36_profile_v2},
    palw_router_binding_v2::{
        PALW_ROUTER_BINDING_TABLE_GDN_V2, PALW_ROUTER_BINDING_VERSION_V2, PalwRoutedReaderBindingV2, palw_routed_readers_v2,
    },
    palw_step_refute::palw_router_family_v2,
};

fn declaration() -> PalwRouterBindingDeclarationV2 {
    let profile = qwen36_profile_v2(QWEN36_35B_A3B).unwrap();
    let readers = palw_routed_readers_v2(&profile)
        .into_iter()
        .map(|(table, reader)| {
            let nodes = if table == PALW_ROUTER_BINDING_TABLE_GDN_V2 { &profile.gdn_nodes } else { &profile.attn_nodes };
            let router = nodes.iter().position(|node| palw_router_family_v2(node.kernel_semantics_id).is_some()).unwrap() as u16;
            PalwRoutedReaderBindingV2 { table, reader, router }
        })
        .collect();
    PalwRouterBindingDeclarationV2 {
        version: PALW_ROUTER_BINDING_VERSION_V2,
        class_id: profile.shape_profile_id(),
        artifact_root: Hash64::from_bytes([3; 64]),
        profile,
        readers,
    }
}

#[test]
fn carrier_uses_existing_tag_and_chain_signature_contract() {
    let key = ValidatorKey::from_seed([0x42; 32]);
    let domain = Hash64::from_bytes([7; 64]);
    let declaration = declaration();
    let object = signed_router_binding_v2(&key, domain, declaration.clone()).unwrap();
    let encoded = borsh::to_vec(&object).unwrap();
    assert_eq!(encoded[0], 244);
    let decoded: PalwConsensusObjectV2 = borsh::from_slice(&encoded).unwrap();
    let PalwConsensusObjectV2::MemoryRoutingV2 { object } = decoded else { panic!("existing native carrier expected") };
    let PalwMemoryRoutingObjectV2::RouterBindingDeclared { declaration: carried, signature } = *object else {
        panic!("declaration expected")
    };
    assert_eq!(carried, declaration);
    let verifies = |domain, declaration: &PalwRouterBindingDeclarationV2, context: &[u8]| {
        kaspa_txscript::verify_mldsa87_with_context(
            key.public_key(),
            palw_router_binding_message_v2(domain, declaration).as_byte_slice(),
            &signature,
            context,
        )
        .unwrap_or(false)
    };
    assert!(verifies(domain, &carried, PALW_ROUTER_BINDING_SIGNATURE_CONTEXT_V2));
    assert!(!verifies(Hash64::from_bytes([8; 64]), &carried, PALW_ROUTER_BINDING_SIGNATURE_CONTEXT_V2));
    assert!(!verifies(domain, &carried, b"another-context"));
    let mut changed = carried;
    changed.artifact_root = Hash64::from_bytes([4; 64]);
    assert!(!verifies(domain, &changed, PALW_ROUTER_BINDING_SIGNATURE_CONTEXT_V2));
}

#[test]
fn incomplete_or_foreign_bindings_are_refused() {
    let key = ValidatorKey::from_seed([0x42; 32]);
    let domain = Hash64::from_bytes([7; 64]);
    let good = declaration();
    let mut missing = good.clone();
    missing.readers.pop();
    assert!(signed_router_binding_v2(&key, domain, missing).is_err());
    let mut foreign = good.clone();
    foreign.class_id = Hash64::from_bytes([9; 64]);
    assert!(signed_router_binding_v2(&key, domain, foreign).is_err());
    let mut after = good;
    after.readers[0].router = after.readers[0].reader;
    assert!(signed_router_binding_v2(&key, domain, after).is_err());
}

#[test]
fn cli_writes_native_carrier_and_preserves_existing_output() {
    let path = std::env::temp_dir().join(format!("misaka-router-binding-sdk-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let input = path.join("declaration.borsh");
    let seed = path.join("seed.hex");
    let output = path.join("object.borsh");
    std::fs::write(&input, borsh::to_vec(&declaration()).unwrap()).unwrap();
    {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&seed).unwrap().write_all("42".repeat(32).as_bytes()).unwrap();
    }
    let args = vec![
        "sign".into(),
        "--network".into(),
        "testnet-12".into(),
        "--declaration".into(),
        input.display().to_string(),
        "--key-file".into(),
        seed.display().to_string(),
        "--out".into(),
        output.display().to_string(),
    ];
    cli::run(&args).unwrap();
    let first = std::fs::read(&output).unwrap();
    assert!(matches!(borsh::from_slice::<PalwConsensusObjectV2>(&first).unwrap(), PalwConsensusObjectV2::MemoryRoutingV2 { .. }));
    assert!(cli::run(&args).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), first);
    std::fs::remove_file(&output).unwrap();
    let mut invalid = declaration();
    invalid.readers.clear();
    std::fs::write(&input, borsh::to_vec(&invalid).unwrap()).unwrap();
    std::fs::remove_file(&seed).unwrap();
    let error = cli::run(&args).unwrap_err();
    assert!(error.contains("exactly the class's routed readers"), "{error}");
    assert!(!output.exists());
    std::fs::remove_dir_all(&path).unwrap();
}
