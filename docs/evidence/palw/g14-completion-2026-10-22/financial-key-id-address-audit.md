# G14 funding / refund / relay key identity audit

Read-only during frozen core build. No repo edits, builds or RPC. Scope prioritizes the new TFO1/funding preflight/escrow candidates, archive ending effects, and their actual native payout consumers; this is not a whole-repository financial correctness claim.

## Result

No additional confirmed unkeyed-key-ID -> native P2PKH destination confusion was found in the inspected candidate paths after the archive agent's current repair. The actual native spend test remains the relevant verification; DTO/helper checks alone did not detect the original bug.

## Distinct identities must remain distinct

- `mldsa87_key_id(pubkey)` is the historical unkeyed signing/validator/holder identity. It must stay unchanged; changing it globally would change old position keys and signed meanings.
- `kaspa_hashes::blake2b_512_address_payload(pubkey)` is the domain-keyed native address value checked by OP_BLAKE2B_512 and supplied to p2pkh_mldsa87_spk when the destination is specifically that key's own address.
- An already authorized payout/refund payload is a destination, not necessarily the current signer's own key hash. It should be passed literally into the script, not rehashed or forcibly equated with the signer ID. Current RFC0035 A explicitly restricts THIS candidate's failed-carrier fallback to the same payer's native address; that local restriction must not be generalized to other authorized refund/payout fields.

## New candidate trace

`palw_court_original_escrow_v1.rs` current code derives BOTH values from the supplied real payer pubkey: verify_signature_v1 matches unkeyed payer_key_id and keyed refund_payload, then verifies the new-domain signature. check_actual_output_v1 separately matches the actual indexed output's amount and strict native P2PKH payload to the keyed payer address. The previous 'key ID equals output payload' path is gone. Terms comments distinguish both. Negative tests explicitly lock a trap output to payer_key_id and require refusal.

`palw_tir_funded_original_envelope_v1.rs` retains payer_key_id only as the signing-key identity during structural/key checks. Actual output inspection delegates to the repaired funding consent API with the literal payer pubkey. Its synthetic fixtures now create keyed native address outputs. `palw_court_funding_preflight_v1.rs` delegates preparation to that same actual carrier/consent binding; it does not separately manufacture a destination from the ID. No new actual native escrow spending/refund installation is claimed by these functions.

The current RFC0035 note explicitly requires the same payer fallback. A signature authorizing some other destination does not meet that candidate's fallback safety premise; this is a per-contract refusal, not a reason to change existing arbitrary authorized payout semantics.

## Archive / relay and existing payout consumers

`palw_court_archive_discharge_v1`, `palw_court_archive_verify_v1`, `palw_court_end_archive_v1` and its fold record/verify unresolved effects and obligations. They do not turn a bond/key ID into a P2PKH destination or implement the still-missing archive financial refund/UTXO installation. Thus there is no second destination conversion in those new candidates to repair, and this audit does NOT establish actual settlement.

The native processor's pending_payouts -> TransactionOutput path (`processor/palw.rs`, palw payout rendering around4469) feeds the retained `PalwPayoutV2.payload` directly into p2pkh_mldsa87_spk, as appropriate for an authorized destination. Existing native held-fee refund (`palw_native_liability_hold_fold_v1::refund_native_fee_v1`) similarly queues row.refund_payload literally. Its producer is `palw_native_job_fold_v39`: requester_payout comes from the requester's retained bond payout_payload, not mldsa87_key_id. These paths must not be 'fixed' by deriving the relay/current signer's own address instead.

Current shared court relays authenticate the current bond public key or producer/court signer but do not create native output destinations. A relay signature/identity must not silently become the beneficiary. Future complete coordinator/refund outputs need the exact contract-authorized payload and an actual spendability/conservation test, especially when beneficiary != relay.

## Follow-up tests that matter

Keep repaired native signed-spend test proving fallback output is spendable by the real payer and the key-ID trap is refused. At actual escrow/financial installation, test signer identity, original payer own fallback address, arbitrary separately authorized beneficiary destination, and independent relay as FOUR distinct roles. Recheck script version/payload and amount once at actual native transaction boundary; preserving a Hash64 in a DTO is not that verification. No global hash/key ID change, old tag change or model review is proposed.
