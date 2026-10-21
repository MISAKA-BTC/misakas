# Tokenizer canonical material -> existing material builder: next concrete unit

Read-only design during frozen verification. No repo edits, build or RPC.

## Actual connection and authentication

Add a sibling private tokenizer-terminal method on the existing SharedTirBuilderStoreV1 (same parent as the new Constraint terminal), not on numeric build_terminal and not by fabricating PalwStagedCertificateRefV1. Its distinct signed intent commits claim/scope/source-policy, immutable claim semantics commitment, target token ID, complete raw/rendering/key-proof material identity and literal filer. This remains a private evidence candidate: paid dispatch, old signed moves and complete original are absent.

The method reconstructs the installed shared context, actual retained frame/source/header and registered class. `palw_offence_attribution_v1::palw_offence_target_v1(state,&claim)` supplies the real target (including claim semantics, execution/job/output identity); a caller-created target is not accepted. `PalwDecodeViewV1::from_tir_header_v1` reconstructs the native view. `palw_tokenizer_statement_view_v1(target,view,original_job)` checks the actual immutable tokenizer/table statement. Unlike Constraint's table-only statement, Tokenizer needs all of token_table_root, rendering_rule, raw_tokenizer_bytes and tokenizer_key_events. Missing any means explicit Missing/Error, never zero.

The actual primitive is `PalwTokenizerTableStreamV1::new_with_operand_length_view_v1(context,target,view,job,id,present,length,table_opening,actual_raw_length)`. The operand length is the supplied complete raw file's real length, NOT silently replaced by signed length: its authenticated EOF may establish a signed length violation. The table leaf opening is checked at construction, rendering bytes at finish; raw flat hash is checked at parser finish. No finding is returned before raw EOF+seal+table finish. A fresh reconstruction recheck must repeat these validations from state/material, as in the new Constraint evidence candidate.

## Material and real acquisition gap

Borrow raw &[u8], one literal target rendering opening+optional bytes, and an ordered slice of compact key insertion proofs. Retain Missing raw, missing rendering item, and missing pending proof as explicit Need/Missing. Authenticated absent rendering is a supplied opening with None bytes, not missing input. Outer material identity commits all bytes/proofs plus target; strict exact consumption rejects extra proofs. Do not use global tokenizer table/file fallback or an owned stream as an availability certificate.

Current seven-family shared TIR source requests provide step/rows/logits/IDs but NO tokenizer raw artifact body or JSON key-proof stream. Thus constructor connection is possible with complete borrowed material, but F03 retrieval/custody/retention for these inputs remains missing. Existing tokenizer inventory certificate authenticates original exact EOF/work bounds, not present possession/delivery of raw or a ready ordered proof history. A new public retention duty/fee/profile is not introduced by this private method.

Production `PalwJsonKeySetV1` verifies proofs; full tree/proof construction exists only in cfg(test) `palw_tokenizer_json_stream_v1::tests::Book`. Therefore do not call a fake zero-proof generator or copy Book into production without bounding its tree memory/IO. First candidate should consume supplied bounded proofs and verify them in the exact native parser pending order. Their validity is against successive parser key_set_root states, not an independently supplied arbitrary root.

## Canonical maximal carrier algorithm, actual existing limits

Use a private child of `palw_tokenizer_table_stream_v1` to access the actual parser and pending_overrun behavior without duplicating its grammar. Constants remain existing: bytes<=65536, proofs<=64, exact Borsh packet bytes<=90000. These are dimensions of one packet, not an adopted total-original capacity.

Maintain a temporary candidate packet and a native probe cloned from the current stream. Starting at actual tokenizer_offset:
1. If a real pending key exists after native pending_overrun, take the NEXT provided compact proof. It must expand and authenticate against the native namespace/key and current key-set root. If its addition would exceed 64 proofs or 90000 serialized bytes, finish the current nonempty packet; the next packet may be proof-only. A first proof that cannot fit alone is explicit bounded refusal (not split or skipped).
2. Otherwise push the longest remaining raw prefix that fits the remaining 65536 byte budget and 90000 packet budget, letting the actual parser stop at its next pending key. Append only its consumed prefix. Canonical prefix may stop at the real parser pause even when byte room remains; then step 1 can add the proof and continue raw in the SAME packet.
3. Finalize on byte/proof/serialized-size exhaustion, raw EOF with no pending key, or inability to add the next whole required proof. Submit that exact packet through the existing `packet` primitive, not merely trust the probe. Empty packets are forbidden; progress must be checked.
4. Repeat, require all raw and all supplied proofs consumed, seal actual tokenizer, stream the target rendering in existing maximum legal rendering chunks, then finish.

Native `pending_overrun` deliberately marks WorkBoundExceeded when a pending proof would exceed the signed bound. That branch can continue without another proof because the actual primitive removed the pending event; do NOT fabricate a proof or continue demanding a proof the native grammar no longer consumes. Extra provided proofs still refuse, so a signed-underbound material provider must supply the actual canonical consumed proof prefix.

For allocation preflight use checked bytes/proof count/expanded-proof depth, material hashing, number of native probe calls and packet output bound before cloning/allocating. A small private iterator that emits/executes one bounded packet at a time avoids Vec<all_packets>. Full raw/rendering ownership, sparse-tree construction by provider, stream clone and native parser scratch remain separately admitted resources. Maximum-moves is not an exact enumeration and no arbitrary legacy MAX_REQUESTS counter is incremented.

## Focused next test

Use a real native program/frame plus prospective immutable semantics and accepted tokenizer inventory; extend the existing narrow constrained fixture helper rather than fake record.program. Nonempty JSON with several model/vocab keys and added tokens must yield genuine compact proof events. Build the provided proof sequence using test-only Book, but production must verify it. Cases: mixed maximal bytes+proof interleave; enough distinct events to cross64 proof boundary; serialized90000 boundary; proof-only continuation after fullbyte packet; missing/foreign/duplicate/noncanonical proof; trailing unused proof; badrawhash; wrongtableleaf; signed-work-bound overrun; distinct filer/context; state unchanged. Compare terminal verdict/material identity with the already added alternative-split primitive tests, NOT old move count/fee/clock/retention equivalence.

A positive native fixture must include registered prospective tokenizer semantics plus real accepted exact-EOF inventory. Table-only Constraint fixture is insufficient and must be rejected explicitly; injecting a 'complete' boolean or editing a retained plan to add Tokenizer stage is not acceptable.
