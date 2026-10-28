# Coherent Tokenizer builder candidate scope (source ready, unbuilt)

New source:
- consensus/core/src/palw_tokenizer_canonical_replay_v1.rs
- consensus/core/src/palw_court_shared_tokenizer_builder_v1.rs
- consensus/core/src/palw_court_shared_tokenizer_native_tests.rs

Changed existing files:
- palw_tokenizer_table_stream_v1.rs: private child declaration + one actual-proof packing/error test.
- palw_court_shared_tir_builder_store_v1.rs: separate tokenizer child declaration.
- palw_tir_public_fold_v1.rs: new test include only.
- palw_tir_court_v1.rs: cfg(test) real tiny-execution helper accepts explicit tokenizer; existing helper passes the original [3;64] tokenizer exactly.
- palw_tir_public_claim_v1.rs: cfg(test) native frame helper accepts optional actual inventory, preserving old None wrappers; new tokenizer fixture.

No original Constraint native test file edits; no lib/generator changes. No cargo/rustc/RPC/commit performed. New standalone source rustfmt passed; git diff --check passed. Shared fixture files were not globally reformatted.

The actual Store method resolves installed shared context/source/frame, actual offence target, immutable prospective semantics and retained accepted EOF certificate; constructs PalwTokenizerTableStreamV1::new_with_operand_length_view_v1 itself; authenticates distinct MLDSA tokenizer intent with current active filer key; feeds complete borrowed raw/real compact proofs/table rendering through canonical primitive; returns private evidence whose recheck reconstructs this entire chain. It does not accept caller-created Court/target/complete flag. Numerical/Constraint intents and old owned certificates are not reused.

The native positive uses a genuine tiny TIR program/class rebuilt with the real raw tokenizer commitment (not the earlier [1,2,3] program fixture). Actual signed inventory offer -> native inventory Part -> Finish creates EOF certificate; it is passed to derive_tir_public_allocation and prepare_native, with immutable prospective tokenizer semantics and nonempty Tokenizer stage. Actual signed history custody pledge -> production frame_semantic_bundle establishes the required dependency before service binding. Original pledge/signature setup remains the existing explicit verified-half fixture; this is not mined admission/payment.

Test filters:
- private_tokenizer_canonical_builder_consumes_real_ordered_proofs_and_authenticates_operands (real >64 proofs, >65536 raw bytes; comparison with existing native atom replay; proof missing/extra/foreign, raw/operand/rendering mismatch, budget and state immutability).
- shared_tokenizer_builder_uses_native_original_eof_and_rechecks_authenticated_material (real original, ordinary Tokenizer stage, actual EOF, current MLDSA intent; positive fresh recheck; valid signatures over wrong scope/inventory/proof sequence/raw/rendering payload all rejected; missing EOF; before-original and hard boundary; unchanged state).
- existing shared_constraint_material_builder_reconstructs_real_native_original_and_rechecks should regress the preserved helper wrappers.
- canonical_material_replay_matches_distinct_legal_packetizations remains useful primitive regression.

Limits:
- No public raw/proof acquisition or persistence/availability duty exists in the seven-family source set. Borrowed input possession is a local builder precondition, not F03 availability completion.
- Native rules/accepting point are private native-caller inputs; all original allocation config identity cannot be reconstructed from NativeCourtRulesV1 alone. No actual native paid dispatcher/escrow install is introduced.
- No full grammar closure, old fee/clock/signed move equivalence, measured CPU profile, complete original or successful public network submission is claimed.
- EOF certificate authenticates raw hash/exact bytes and native-bounded planning. It does not authenticate a supplied key-proof history; every proof is verified against actual parser pending namespace/key/current root.
