use kaspa_consensus_core::{
    Hash64,
    config::params::palw_t12_shipped_params,
    constants::TX_VERSION,
    hashing::{
        sighash::{
            Mldsa87SigHashReusedValuesUnsync, SigHashReusedValuesUnsync,
            calc_mldsa87_signature_hash,
        },
        sighash_type::SIG_HASH_ALL,
    },
    mass::{MassCalculator, UtxoPlurality, transaction_estimated_serialized_size},
    mldsa87_primitives::{MLDSA87_SIGNATURE_LEN, p2pkh_mldsa87_spk},
    palw_fp_constraint_job_v1::palw_token_table_file_decode_v1,
    palw_mode_v2::PALW_MIRRORED_STANDARD_TX_MASS,
    palw_state_v2::PalwBondKeyV2,
    palw_tokenizer_registration_carriage_v1::encode_tokenizer_registration_payload_v1,
    palw_tokenizer_registration_v1::*,
    palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1,
    palw_tokenizer_rendering_v2::{
        PalwTfp1TokenizerIntakePinsV2, PalwTokenizerRenderingStatementV2,
    },
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::{
        PopulatedTransaction, Transaction, TransactionInput, TransactionOutpoint,
        TransactionOutput, UtxoEntry,
    },
};
use kaspa_txscript::{
    MLDSA87_TX_CONTEXT, TxScriptEngine, caches::Cache, script_builder::ScriptBuilder,
};
use libcrux_ml_dsa::ml_dsa_87;
use std::{env, fs, thread};

fn sign_control(
    keys: &ml_dsa_87::MLDSA87KeyPair,
    digest: Hash64,
    domain: &[u8],
    randomness: u8,
) -> [u8; MLDSA87_SIGNATURE_LEN] {
    *ml_dsa_87::sign(
        &keys.signing_key,
        digest.as_bytes().as_slice(),
        domain,
        [randomness; 32],
    )
    .unwrap()
    .as_ref()
}

fn run() {
    let mut args = env::args_os().skip(1);
    let raw = fs::read(args.next().expect("tokenizer.json path")).unwrap();
    let table =
        palw_token_table_file_decode_v1(&fs::read(args.next().expect("palwttb path")).unwrap())
            .unwrap();
    let eog = table.eog_opening().unwrap();
    let table_root = table.root();
    assert!(eog.verifies(&table_root, table.vocab()));
    let geometry = TokenizerRegistrationGeometryV1::new(raw.len() as u64).unwrap();

    // This policy digest is a synthetic wire identity. Mass depends on its
    // encoded size; no policy is adopted or quoted by this measurement.
    let h = Hash64::from_u64_word;
    let control_keys = ml_dsa_87::generate_key_pair([0x31; 32]);
    let payer_keys = ml_dsa_87::generate_key_pair([0x52; 32]);
    let body = TokenizerRegistrationOpenBodyV1 {
        version: TOKENIZER_REGISTRATION_VERSION_V1,
        network: h(1),
        policy: h(2),
        pins: PalwTfp1TokenizerIntakePinsV2 {
            class_id: h(3),
            container_root: h(4),
            semantics_id: h(5),
            inventory_id: h(6),
        },
        artifact_root: h(7),
        statement: PalwTokenizerRenderingStatementV2 {
            tokenizer_id: table.tokenizer_id,
            exact_raw_bytes: raw.len() as u64,
            table_root,
            vocab: table.vocab(),
            rule: PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
        },
        eog_digest: TokenizerRegistrationFinishV1::eog_digest_v1(&eog).unwrap(),
        opener: PalwBondKeyV2(TransactionOutpoint::new(h(8), 0)),
        nonce: h(9),
        accept_until_daa: 101,
        finish_until_daa: 200,
        material_until_daa: 220,
        max_registration_charge: 1,
    };
    let mut open = TokenizerRegistrationOpenV1 {
        body,
        signature: [0; MLDSA87_SIGNATURE_LEN],
    };
    open.signature = sign_control(
        &control_keys,
        open.signing_message_v1().unwrap(),
        TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1,
        10,
    );
    open.verify_signature_v1(control_keys.verification_key.as_ref())
        .unwrap();
    let open_id = body.id_v1().unwrap();

    let make_part = |index: u32, randomness| {
        let (offset, len) = geometry.part_v1(index).unwrap();
        let offset = offset as usize;
        let bytes = raw[offset..offset + len].to_vec();
        let mut part = TokenizerRegistrationPartV1 {
            body: TokenizerRegistrationPartBodyV1 {
                version: body.version,
                network: body.network,
                policy: body.policy,
                open_id,
                opener: body.opener,
                index,
                offset: offset as u64,
                payload_digest: TokenizerRegistrationPartV1::payload_digest_v1(&bytes).unwrap(),
            },
            signature: [0; MLDSA87_SIGNATURE_LEN],
            bytes,
        };
        part.check_open_v1(&body).unwrap();
        part.signature = sign_control(
            &control_keys,
            part.signing_message_v1().unwrap(),
            TOKENIZER_REGISTRATION_PART_CONTEXT_V1,
            randomness,
        );
        part.verify_signature_v1(control_keys.verification_key.as_ref())
            .unwrap();
        TokenizerRegistrationWireV1::Part(part)
    };

    println!(
        "tokenizer_raw={} parts={} full_part={} last_part={} vocab={} eog_ids={} eog_siblings={} table_root={}",
        raw.len(),
        geometry.part_count,
        geometry.part_v1(0).unwrap().1,
        geometry.part_v1(geometry.part_count - 1).unwrap().1,
        table.vocab(),
        eog.ids.len(),
        eog.opening.siblings.len(),
        table_root
    );
    let mut finish = TokenizerRegistrationFinishV1 {
        body: TokenizerRegistrationFinishBodyV1 {
            version: body.version,
            network: body.network,
            policy: body.policy,
            open_id,
            opener: body.opener,
            eog_digest: body.eog_digest,
            finish_until_daa: body.finish_until_daa,
        },
        signature: [0; MLDSA87_SIGNATURE_LEN],
        eog,
    };
    finish.signature = sign_control(
        &control_keys,
        finish.signing_message_v1().unwrap(),
        TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1,
        13,
    );
    finish
        .verify_signature_v1(control_keys.verification_key.as_ref())
        .unwrap();

    let params = palw_t12_shipped_params();
    assert_eq!(params.max_block_mass, 500_000);
    assert_eq!(PALW_MIRRORED_STANDARD_TX_MASS, 480_000);
    println!(
        "net={} compute_tx_byte={} compute_spk_byte={} sig_op_mass={} transient_byte={} storage_parameter={} block_cap={} relay_cap={}",
        params.net,
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        kaspa_consensus_core::constants::TRANSIENT_BYTE_TO_MASS_FACTOR,
        params.storage_mass_parameter,
        params.max_block_mass,
        PALW_MIRRORED_STANDARD_TX_MASS
    );
    let calculator = MassCalculator::new_with_consensus_params(&params);
    let payer_pk = payer_keys.verification_key.as_ref();
    let spk = p2pkh_mldsa87_spk(&kaspa_hashes::blake2b_512_address_payload(payer_pk).as_bytes());
    // One funded non-coinbase UTXO, one positive same-script change output,
    // one sigop. The sample difference is strictly a mass test, not a quote.
    let input_sompi = 1_000_000_000;
    let output_sompi = 999_000_000;
    let entry = UtxoEntry::new(input_sompi, spk.clone(), 0, false);
    println!(
        "inputs=1 outputs=1 input_sompi={input_sompi} output_sompi={output_sompi} input_coinbase={} spk_bytes={} input_plurality={}",
        entry.is_coinbase,
        spk.script().len(),
        entry.plurality()
    );
    println!(
        "carrier,payload_bytes,unlock_bytes,output_bytes,estimated_tx_bytes,compute,transient,storage,max,relay_margin,block_margin"
    );
    for (index, (name, wire)) in [
        ("Open", TokenizerRegistrationWireV1::Open(open)),
        ("Part/max", make_part(0, 11)),
        ("Part/tail", make_part(geometry.part_count - 1, 12)),
        ("Finish", TokenizerRegistrationWireV1::Finish(finish)),
    ]
    .into_iter()
    .enumerate()
    {
        let payload = encode_tokenizer_registration_payload_v1(&wire).unwrap();
        let mut tx = Transaction::new(
            TX_VERSION,
            vec![TransactionInput::new(
                TransactionOutpoint::new(h(50 + index as u64), 0),
                Vec::new(),
                0,
                1,
            )],
            vec![TransactionOutput::new(output_sompi, spk.clone())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        let digest = calc_mldsa87_signature_hash(
            &PopulatedTransaction::new(&tx, vec![entry.clone()]),
            0,
            SIG_HASH_ALL,
            &Mldsa87SigHashReusedValuesUnsync::new(),
        );
        let mut sig_item =
            sign_control(&payer_keys, digest, MLDSA87_TX_CONTEXT, 30 + index as u8).to_vec();
        sig_item.push(SIG_HASH_ALL.to_u8());
        tx.inputs[0].signature_script = ScriptBuilder::new()
            .add_data(&sig_item)
            .unwrap()
            .add_data(payer_pk)
            .unwrap()
            .drain();
        tx.finalize();
        let populated = PopulatedTransaction::new(&tx, vec![entry.clone()]);
        TxScriptEngine::from_transaction_input(
            &populated,
            &tx.inputs[0],
            0,
            &populated.entries[0],
            &SigHashReusedValuesUnsync::new(),
            &Cache::new(4),
        )
        .execute()
        .unwrap();
        let contextless = calculator.calc_non_contextual_masses(&tx);
        let contextual = calculator.calc_contextual_masses(&populated).unwrap();
        let max = contextual.max(contextless);
        println!(
            "{name},{},{},{},{},{},{},{},{max},{},{}",
            tx.payload.len(),
            tx.inputs[0].signature_script.len(),
            kaspa_consensus_core::mass::transaction_output_estimated_serialized_size(
                &tx.outputs[0]
            ),
            transaction_estimated_serialized_size(&tx),
            contextless.compute_mass,
            contextless.transient_mass,
            contextual.storage_mass,
            i128::from(PALW_MIRRORED_STANDARD_TX_MASS) - i128::from(max),
            i128::from(params.max_block_mass) - i128::from(max)
        );
    }
}

fn main() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}
