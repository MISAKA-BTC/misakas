//! Funded lifecycle carrier construction and fence-aware rent. Node policy only.
use super::*;

/// Match acceptance and coinbase rent at the carrier's current virtual DAA. Unsupported lifecycle
/// kinds and inactive rent fences add nothing; this never arms a consensus rule.
pub(super) fn palw_lifecycle_carrier_rent_v2(
    params: &kaspa_consensus_core::config::params::Params,
    object: &PalwConsensusObjectV2,
    daa: u64,
) -> u64 {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return 0;
    };
    if !(params.palw_certification_rent.is_some_and(|f| f.is_active(daa)) || params.palw_model_lines_active_at(daa))
        || !params.palw_lifecycle_kind_fences_v1().kind_in_force_at(object, daa)
    {
        return 0;
    }
    kaspa_consensus_core::palw_state_v2::palw_object_rent_ceiling_v3(
        object,
        params.palw_offence_attribution_active_at(daa),
        bundle.state.capacity_batch_active_at(daa),
        bundle.state.legacy_public_filer_active_at(daa),
    )
}

/// A standard carrier's largest relay fee plus this object's rent and a positive change output.
/// Keep the legacy float floor for ordinary moves; do not make cheap moves reserve a heavy proof's
/// rent. The caller rechecks the actual signed carrier's price before it spends.
pub(super) fn palw_lifecycle_funding_minimum_v2(
    params: &kaspa_consensus_core::config::params::Params,
    object: &PalwConsensusObjectV2,
    daa: u64,
) -> u64 {
    palw_lifecycle_carrier_rent_v2(params, object, daa)
        .saturating_add(relay_fee_for_compute_mass(MAXIMUM_STANDARD_TRANSACTION_MASS))
        .saturating_add(1)
        .max(palw_fee_funding_floor_v1())
}

/// Shared production signer. Factored from the panel service so its real signed fee/change and
/// signature can be checked without injecting a consensus service or a running mempool.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_lifecycle_carrier_v2(
    config: &Config,
    kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    object: &PalwConsensusObjectV2,
    funding_outpoint: TransactionOutpoint,
    funding: &UtxoEntry,
    extra_outputs: &[TransactionOutput],
    replaces_feerate: Option<f64>,
    daa: u64,
) -> Result<Transaction, String> {
    // **Refuse before signing, and name the field.**
    //
    // The one input is signed with this node's key, so a funding output that does not pay to
    // this key's own script produces a carrier that cannot be spent by anybody who could have
    // built it. The mempool's word for that is `script ran, but verification failed` — the
    // script engine's generic verdict, which names a signature and so sends every reader to
    // `--palw-producer-key` and `--palw-producer-pay-address`, the two things a first
    // registration has already got right. Checking it here is free, it happens before the
    // ML-DSA-87 signature is computed, and it can say WHICH of the two scripts it is holding.
    //
    // It sits in the shared builder rather than in the registration path because every
    // carrier — receipt, class, court — spends the same way and fails the same way.
    let signable = signable_script(kp.verification_key.as_ref());
    if funding.script_public_key != signable {
        let addr = |spk: &kaspa_consensus_core::tx::ScriptPublicKey| {
            kaspa_txscript::extract_script_pub_key_address(spk, config.prefix())
                .map(|a| a.to_string())
                .unwrap_or_else(|_| "an address this node cannot render".to_string())
        };
        return Err(format!(
            "the funding output pays to {} and --palw-producer-key signs for {} — this node cannot spend it. \
                 Nothing was signed. Fund the key's own address, or point --palw-producer-key at the key that owns \
                 this output; a state dir carried over from a run with a different key is the usual source of a \
                 remembered outpoint belonging to neither.",
            addr(&funding.script_public_key),
            addr(&signable)
        ));
    }
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
        .map_err(|e| format!("the lifecycle payload does not serialize: {e}"))?;
    let params = &config.params;
    let mass_calculator = MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    );

    // Two passes: the fee depends on the mass, and the mass on the (fixed-size) signature. A
    // dummy signature of the real length prices the transaction, then the real one replaces it.
    let locked = extra_outputs.iter().try_fold(0u64, |sum, o| sum.checked_add(o.value)).ok_or("locked outputs overflow")?;
    let build = |fee: u64, signature_script: Vec<u8>| -> Result<Transaction, String> {
        // The collateral is spent as well as the fee, and saying so by name is the difference
        // between "fund the address again" and an operator wondering why a bond they have the
        // money for will not register.
        let needed = fee.checked_add(locked).ok_or("carrier funding overflows")?;
        if funding.amount <= needed {
            return Err(format!(
                "funding UTXO holds {} sompi; this carrier needs {fee} fee + {locked} locked — fund the address again",
                funding.amount
            ));
        }
        let mut input = TransactionInput::new(funding_outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
        input.signature_script = signature_script;
        let mut outputs = extra_outputs.to_vec();
        outputs.push(TransactionOutput::new(funding.amount - needed, funding.script_public_key.clone()));
        Ok(Transaction::new(TX_VERSION, vec![input], outputs, 0, SUBNETWORK_ID_PALW_LIFECYCLE.clone(), 0, payload.clone()))
    };

    let dummy_sig_script = {
        let sig = vec![0u8; kaspa_txscript::MLDSA87_SIG_LEN + 1];
        kaspa_txscript::script_builder::ScriptBuilder::new()
            .add_data(&sig)
            .and_then(|b| b.add_data(kp.verification_key.as_ref()))
            .map(|b| b.drain())
            .map_err(|e| format!("sig script shape: {e}"))?
    };
    let priced = build(1, dummy_sig_script)?;
    let masses = mass_calculator.calc_non_contextual_masses(&priced);
    // Rent is burned; leave the full relay carriage fee for the miner as well.
    let rent = palw_lifecycle_carrier_rent_v2(params, object, daa);
    let fee = relay_fee_for_compute_mass(masses.compute_mass).checked_add(rent).ok_or("carrier fee overflows")?;
    // V01's panel side: a carrier on the input of our own stuck tip carrier replaces it, so it
    // pays above it (`PalwCarrierReplacementV1::floor_for`: that input only, while the tick's
    // opening stands).
    let fee = match replaces_feerate {
        // The pool compares feerates over the widest mass it knows — storage mass included.
        Some(rate) => {
            let storage = mass_calculator
                .calc_contextual_masses(&MutableTransaction::with_entries(priced.clone(), vec![funding.clone()]).as_verifiable())
                .map(|c| c.storage_mass)
                .unwrap_or(0);
            palw_replacement_fee_v1(fee, rate, masses.max().max(storage))
        }
        None => fee,
    };

    let unsigned = build(fee, vec![])?;
    let mtx = MutableTransaction::with_entries(unsigned, vec![funding.clone()]);
    let reused = Mldsa87SigHashReusedValuesUnsync::new();
    let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &reused);
    let mut sig_data = libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0u8; 32])
        .map_err(|e| format!("ML-DSA-87 sign: {e:?}"))?
        .as_ref()
        .to_vec();
    sig_data.push(SIG_HASH_ALL.to_u8());
    let signature_script = kaspa_txscript::script_builder::ScriptBuilder::new()
        .add_data(&sig_data)
        .and_then(|b| b.add_data(kp.verification_key.as_ref()))
        .map(|b| b.drain())
        .map_err(|e| format!("sig script: {e}"))?;
    let mut tx = mtx.tx;
    tx.inputs[0].signature_script = signature_script;
    tx.finalize();
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::config::params::{ForkActivation, Params};
    use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_consensus_core::palw_offence_attribution_v1::{PALW_EXECUTOR_REFUTED_VERSION_V1, PalwExecutorRefutedEvidenceV1};
    use kaspa_consensus_core::palw_offence_v1::{PalwOffenceKindV1, PalwPanelContradictionV1, PalwPromptProofV1};
    use kaspa_consensus_core::tx::VerifiableTransaction;

    fn config() -> Config {
        let mut p = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        p.palw_model_lines = None; // Isolate the certification rent clock in this fixture.
        p.palw_certification_rent = Some(ForkActivation::new(100));
        p.palw_legacy_public_filer_v1 = Some(ForkActivation::new(200));
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode else { panic!() };
        bundle.state = bundle.state.clone().with_legacy_public_filer_from_daa(Some(200));
        Config::new(p)
    }

    fn op(n: u64) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_u64_word(n), 0)
    }

    /// Fee tests authenticate only transaction spends, not a model execution or this evidence's
    /// admissibility. The pricing reader intentionally prices declared work even on a failing proof.
    fn whole(fp: bool) -> PalwConsensusObjectV2 {
        use kaspa_consensus_core::palw_attempt_rules_v1::{palw_attempt_context_v1, palw_canonical_checkpoint_profile_v1};
        use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
        use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2};
        let mut profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        profile.n_ctx = 2_097_152;
        let job_context = palw_attempt_context_v1(&profile, &Hash64::from_u64_word(3), (262_143, 2), Hash64::default());
        let binding = PalwStepBindingV2 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            checkpoint_profile: palw_canonical_checkpoint_profile_v1(&profile),
            state_chunk_map_id: profile.state_chunk_map_id,
            shape_profile: profile,
            job_context,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            step_leaf_count: 64,
            step_merkle_root: Hash64::default(),
            checkpoint_count: 0,
            checkpoint_merkle_root: Hash64::default(),
            committed_execution_root: Hash64::default(),
        };
        let proof = if fp {
            use kaspa_consensus_core::palw_freeprompt_v3::*;
            PalwPromptProofV1::FpCanonicalWhole {
                job: Box::new(PalwFreePromptJobV3 {
                    version: PALW_FP_V3_VERSION,
                    network_domain: Hash64::default(),
                    class_id: Hash64::default(),
                    executor_bond: op(9),
                    executor_pubkey: vec![],
                    operator_id: Hash64::default(),
                    anchor_block: Hash64::default(),
                    anchor_daa: 0,
                    job_nonce: [0; 32],
                    tokenizer_id: Hash64::default(),
                    prompt_token_ids_hash: Hash64::default(),
                    prompt_tokens: 262_143,
                    decode_token_limit: 2,
                    max_context_tokens: 2_097_152,
                    privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
                    prompt_mode: PALW_FP_PROMPT_MODE_CANONICAL,
                    sampling_seed: [0; 32],
                    temperature_q: 0,
                    decode: None,
                    tail: None,
                }),
            }
        } else {
            PalwPromptProofV1::Whole
        };
        let evidence = borsh::to_vec(&PalwExecutorRefutedEvidenceV1 {
            version: PALW_EXECUTOR_REFUTED_VERSION_V1,
            claim_id: Hash64::from_u64_word(3),
            contradiction: PalwPanelContradictionV1::PromptNotAnchored { binding, proof },
            prompt_ids_opening: None,
            reporter_reveal: vec![],
        })
        .unwrap();
        PalwConsensusObjectV2::ObjectiveOffence {
            kind: PalwOffenceKindV1::ExecutorRefuted,
            accused: PalwBondKeyV2(op(9)),
            evidence_id: Hash64::default(),
            evidence,
        }
    }

    fn verify_spend(tx: Transaction, entry: UtxoEntry) {
        let mutable = MutableTransaction::with_entries(tx, vec![entry]);
        let view = mutable.as_verifiable();
        let reused = SigHashReusedValuesUnsync::new();
        let cache = kaspa_txscript::caches::Cache::new(32);
        kaspa_txscript::TxScriptEngine::from_transaction_input(&view, &view.inputs()[0], 0, view.utxo(0).unwrap(), &reused, &cache)
            .with_script_policy(kaspa_txscript::ScriptPolicy::PQ_ONLY)
            .execute()
            .expect("actual ML-DSA-87 spend verifies");
    }

    #[test]
    fn signed_whole_carriers_pay_burned_rent_and_leave_relay_carriage() {
        let cfg = config();
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xFE; 32]);
        for fp in [false, true] {
            let object = whole(fp);
            let rent = palw_lifecycle_carrier_rent_v2(&cfg.params, &object, 200);
            assert_eq!(rent, 13_107_150);
            let minimum = palw_lifecycle_funding_minimum_v2(&cfg.params, &object, 200);
            let entry = UtxoEntry::new(minimum, signable_script(kp.verification_key.as_ref()), 0, false);
            let tx = build_lifecycle_carrier_v2(&cfg, &kp, &object, op(1), &entry, &[], None, 200).unwrap();
            let calc = MassCalculator::new(
                cfg.params.mass_per_tx_byte,
                cfg.params.mass_per_script_pub_key_byte,
                cfg.params.mass_per_sig_op,
                cfg.params.storage_mass_parameter,
            );
            let mass = calc.calc_non_contextual_masses(&tx).compute_mass;
            assert!(mass <= MAXIMUM_STANDARD_TRANSACTION_MASS);
            let fee = entry.amount - tx.outputs[0].value;
            assert_eq!(fee - rent, relay_fee_for_compute_mass(mass), "rent cannot consume the miner's carriage fee");
            let decoded = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload).unwrap();
            assert_eq!(decoded.object, object);
            verify_spend(tx, entry);
        }
    }

    #[test]
    fn inactive_rent_and_fp_fences_keep_ordinary_pricing() {
        let mut cfg = config();
        let old = whole(false);
        let fp = whole(true);
        assert_eq!(palw_lifecycle_carrier_rent_v2(&cfg.params, &old, 99), 0);
        assert_eq!(palw_lifecycle_carrier_rent_v2(&cfg.params, &old, 100), 13_107_150);
        assert_eq!(palw_lifecycle_carrier_rent_v2(&cfg.params, &fp, 199), 0);
        assert_eq!(palw_lifecycle_carrier_rent_v2(&cfg.params, &fp, 200), 13_107_150);
        cfg.params.palw_offence_attribution = None;
        assert_eq!(palw_lifecycle_carrier_rent_v2(&cfg.params, &old, 200), 0);
        assert_eq!(palw_lifecycle_carrier_rent_v2(&cfg.params, &fp, 200), 0);
        cfg.params.palw_certification_rent = Some(ForkActivation::never());
        cfg.params.palw_model_lines = None;
        assert_eq!(palw_lifecycle_carrier_rent_v2(&cfg.params, &old, u64::MAX), 0);
        assert_eq!(palw_lifecycle_funding_minimum_v2(&cfg.params, &old, 200), palw_fee_funding_floor_v1());
    }

    #[test]
    fn panel_whole_pricing_reads_the_batch_receipt_fence_as_the_processor_does() {
        use kaspa_consensus_core::palw_offence_attribution_v1::{
            PALW_PANEL_FALSE_VALID_VERSION_V2, PalwFalseValidReceiptV1 as R, PalwPanelFalseValidEvidenceV2,
        };
        use kaspa_consensus_core::palw_panel_v2::{PalwSeatReceiptV2, PalwSeatReceiptV3};
        let mut cfg = config();
        cfg.params.palw_capacity_batch_licence = Some(ForkActivation::new(300));
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut cfg.params.palw_consensus_mode else {
            panic!()
        };
        bundle.state = bundle.state.clone().with_capacity_verify_mirrors(None, Some(300));
        let full = PalwSeatReceiptV2 {
            claim: Hash64::from_u64_word(3),
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: PalwBondKeyV2(op(9)),
            signed_daa: 200,
            signature: vec![],
        };
        let windowed = kaspa_consensus_core::palw_batch_licence_v1::PalwWindowedReceiptV1 {
            receipt: PalwSeatReceiptV3 {
                receipt: full.clone(),
                segments: kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2::full(2),
            },
            anchor_hash: Hash64::default(),
            from_daa: 200,
            to_daa: 200,
            count: 1,
            leaf_index: 0,
            path: vec![],
        };
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xF3; 32]);
        for fp in [false, true] {
            let PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = whole(fp) else { panic!() };
            let ex: PalwExecutorRefutedEvidenceV1 = borsh::from_slice(&evidence).unwrap();
            for receipt in [R::Full(full.clone()), R::Windowed(windowed.clone())] {
                let is_windowed = matches!(receipt, R::Windowed(_));
                let evidence = borsh::to_vec(&PalwPanelFalseValidEvidenceV2 {
                    version: PALW_PANEL_FALSE_VALID_VERSION_V2,
                    claim_id: ex.claim_id,
                    accused_seat: op(9),
                    receipt,
                    contradiction: ex.contradiction.clone(),
                    prompt_ids_opening: None,
                    reporter_reveal: vec![],
                })
                .unwrap();
                let object = PalwConsensusObjectV2::ObjectiveOffence {
                    kind: PalwOffenceKindV1::PanelFalseValidV2,
                    accused: PalwBondKeyV2(op(9)),
                    evidence_id: Hash64::default(),
                    evidence,
                };
                assert_eq!(palw_lifecycle_carrier_rent_v2(&cfg.params, &object, 299), if is_windowed { 0 } else { 13_107_150 });
                assert_eq!(palw_lifecycle_carrier_rent_v2(&cfg.params, &object, 300), 13_107_150);
                let entry = UtxoEntry::new(30_000_000, signable_script(kp.verification_key.as_ref()), 0, false);
                let tx = build_lifecycle_carrier_v2(&cfg, &kp, &object, op(1), &entry, &[], None, 300).unwrap();
                assert!(entry.amount - tx.outputs[0].value > 13_107_150);
                verify_spend(tx, entry);
            }
        }
    }

    #[test]
    fn heavy_filing_skips_relay_residue_young_locked_and_busy_money() {
        let cfg = config();
        let object = whole(true);
        let minimum = palw_lifecycle_funding_minimum_v2(&cfg.params, &object, 200);
        assert_eq!(minimum, 19_107_151);
        let coin = |amount, daa, cb| UtxoEntry::new(amount, Default::default(), daa, cb);
        let locked: HashSet<_> = [op(4)].into_iter().collect();
        let usable = |o: &TransactionOutpoint, e: &UtxoEntry| palw_fee_funding_usable_v1(o, e, 1_600, 600, Some(op(5)), &locked);
        let free = |o: &TransactionOutpoint| *o != op(6);
        let residue = coin(palw_fee_funding_floor_v1(), 0, false);
        assert!(palw_fee_funding_pays_v1(residue.amount), "the old funding rule would keep offering it");
        let mut scan = PalwFeeFundingScanV1 { minimum, ..Default::default() };
        for (n, e) in [
            (1, residue.clone()),
            (2, coin(minimum, 1_001, true)),
            (4, coin(minimum * 2, 0, false)),
            (5, coin(minimum * 3, 0, false)),
            (6, coin(minimum * 4, 0, false)),
            (7, coin(minimum - 1, 0, false)),
        ] {
            scan.offer(op(n), e, usable, free);
        }
        assert!(scan.found.is_none());
        scan.offer(op(3), coin(minimum, 1_000, true), usable, free);
        assert_eq!(scan.found.unwrap().0, op(3), "a 600-DAA mature output funds this proof");
        let mut cheap = PalwFeeFundingScanV1::default();
        cheap.offer(op(1), residue, usable, free);
        assert_eq!(cheap.found.unwrap().0, op(1), "a later cheap move has no sticky heavy floor");
    }

    #[test]
    fn priced_signer_checks_rent_replacement_and_locked_outputs_before_signing() {
        let cfg = config();
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xFB; 32]);
        let object = whole(true);
        let script = signable_script(kp.verification_key.as_ref());
        let residue = UtxoEntry::new(palw_fee_funding_floor_v1(), script.clone(), 0, false);
        assert!(build_lifecycle_carrier_v2(&cfg, &kp, &object, op(1), &residue, &[], None, 200).unwrap_err().contains("fee"));
        let entry = UtxoEntry::new(1_000_000_000, script.clone(), 0, false);
        let extra = TransactionOutput::new(20_000_000, script);
        let base = build_lifecycle_carrier_v2(&cfg, &kp, &object, op(1), &entry, std::slice::from_ref(&extra), None, 200).unwrap();
        let bumped =
            build_lifecycle_carrier_v2(&cfg, &kp, &object, op(1), &entry, std::slice::from_ref(&extra), Some(1_000.0), 200).unwrap();
        assert_eq!(bumped.outputs[0], extra);
        assert!(bumped.outputs[1].value < base.outputs[1].value);
        verify_spend(bumped, entry.clone());
        let overflow = [TransactionOutput::new(u64::MAX, Default::default()), TransactionOutput::new(1, Default::default())];
        assert!(build_lifecycle_carrier_v2(&cfg, &kp, &object, op(1), &entry, &overflow, None, 200).unwrap_err().contains("overflow"));
        let foreign = UtxoEntry::new(100_000_000, Default::default(), 0, false);
        assert!(build_lifecycle_carrier_v2(&cfg, &kp, &object, op(1), &foreign, &[], None, 200).unwrap_err().contains("cannot spend"));
    }
}
