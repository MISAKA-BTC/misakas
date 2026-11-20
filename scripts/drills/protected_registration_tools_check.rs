//! Offline actual-input check of setup objects and candidate parent pins.
//! A signed synthetic EOF offer is not a native accepted inventory.
use kaspa_consensus_core::{
    Hash64,
    config::drill::{PalwDrillSaltV1, palw_chain_params_v1},
    palw_attempt_v2::palw_network_domain_v2_for,
    palw_class_semantics_v1::{
        PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1, PalwClassSemanticsDeclarationV1, palw_class_semantics_message_v1,
    },
    palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    palw_tir_admission_v1::palw_tir_class_record_v1,
    palw_tir_container_v1::{PALW_TIR_CONTAINER_BINDING_SIGNATURE_CONTEXT_V1, palw_tir_container_binding_message_v1},
    palw_tokenizer_artifact_inventory_v1::{PALW_ARTIFACT_INVENTORY_CONTEXT_V1, PalwTokenizerArtifactInventoryOfferV1},
    palw_tokenizer_registration_v1::TokenizerRegistrationPolicyV1,
    tx::TransactionOutpoint,
};
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed};
use misaka_palw_sdk::lineages::tir::TirLineageV1;
use std::{fs, io::Write, path::Path};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 8, "artifact semantics setup key salt policy new-offer");
    let entry = TirLineageV1::open_entry(Path::new(&args[1])).unwrap();
    let semantics: PalwClassSemanticsDeclarationV1 = borsh::from_slice(&fs::read(&args[2]).unwrap()).unwrap();
    let setup = Path::new(&args[3]);
    let key = ValidatorKey::from_seed(load_validator_seed(&args[4]).unwrap());
    let salt = PalwDrillSaltV1::from_hex(&args[5]).unwrap();
    let params = palw_chain_params_v1("testnet-12".parse().unwrap(), Some(&salt)).unwrap();
    let domain = palw_network_domain_v2_for(params.net.to_string().as_bytes(), Some(params.genesis.hash));
    let object = |name: &str| -> PalwConsensusObjectV2 { borsh::from_slice(&fs::read(setup.join(name)).unwrap()).unwrap() };
    match object("class-layout.object") {
        PalwConsensusObjectV2::TirClassLayoutPublishedV1 { class_id, layout } => {
            assert_eq!(class_id, entry.class_id());
            assert_eq!(*layout, entry.class.layout);
        }
        _ => panic!("layout publication absent"),
    }
    match object("class-semantics.object") {
        PalwConsensusObjectV2::ClassSemanticsDeclaredV1 { declaration, signature } => {
            assert_eq!(borsh::to_vec(&*declaration).unwrap(), borsh::to_vec(&semantics).unwrap());
            assert!(palw_staged_verify_mldsa87_v1(
                key.public_key(),
                palw_class_semantics_message_v1(domain, &declaration).as_byte_slice(),
                &signature,
                PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1
            ));
        }
        _ => panic!("semantic declaration absent"),
    }
    let binding = match object("container-binding.object") {
        PalwConsensusObjectV2::TirClassContainerBoundV1 { binding, signature } => {
            assert!(palw_staged_verify_mldsa87_v1(
                key.public_key(),
                palw_tir_container_binding_message_v1(domain, &binding).as_byte_slice(),
                &signature,
                PALW_TIR_CONTAINER_BINDING_SIGNATURE_CONTEXT_V1
            ));
            binding
        }
        _ => panic!("container binding absent"),
    };
    let mut offer = PalwTokenizerArtifactInventoryOfferV1 {
        version: 1,
        artifact_hash: semantics.tokenizer_id,
        opener: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(59), 0)),
        maximum_bytes: semantics.raw_tokenizer_bytes,
        accept_until_daa: 30_000,
        signature: Vec::new(),
    };
    offer.signature = key.sign_with_context(offer.id_v1().as_byte_slice(), PALW_ARTIFACT_INVENTORY_CONTEXT_V1).to_vec();
    offer.validate_signature_v1(key.public_key(), palw_staged_verify_mldsa87_v1).unwrap();
    fs::OpenOptions::new().write(true).create_new(true).open(&args[7]).unwrap().write_all(&borsh::to_vec(&offer).unwrap()).unwrap();
    let policy: TokenizerRegistrationPolicyV1 = borsh::from_slice(&fs::read(&args[6]).unwrap()).unwrap();
    let (record, _) = palw_tir_class_record_v1(&entry.class, &entry.artifact_root).unwrap();
    println!(
        "{}",
        serde_json::json!({"setupObjectsAuthenticated":true,"nativeAdmission":false,"nativeInventoryAccepted":false,
        "partialSpecification": {"version":1,"network":domain.to_string(),"policy_id":policy.id_v1().unwrap().to_string(),
        "class_id":entry.class_id().to_string(),"container_root":binding.container_root.to_string(),
        "semantics_id":semantics.as_bound_row(20_792).commitment().to_string(),"inventory_id":offer.id_v1().to_string(),
        "artifact_root":entry.artifact_root.to_string(),"tokenizer_id":semantics.tokenizer_id.to_string(),
        "table_root":semantics.token_table_root.to_string(),"vocab":record.logits_vocab,
        "rendering_rule":borsh::to_vec(&semantics.rendering_rule).unwrap()[0],
        "opener_transaction_id":offer.opener.0.transaction_id.to_string(),"opener_index":0,
        "declared_key_events":semantics.tokenizer_key_events}})
    );
}
