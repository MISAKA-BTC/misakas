# RFC-0038 producer and seat accounting-answer binding — 2026-10-24

Status: focused runtime verification passed: core 10/10, node seat 5/5, PQ 1/1, staging 1/1 and funded diagnostic 1/1. Final gateway/node metadata verification also passed; the scoped commands and logs are recorded below. This is neither activation evidence nor an executed RFC-0012 campaign. All 15 campaign gates remain UNKNOWN; formal K3 specifications, manifest, weights and signed execution inputs have not been created.

## Connected consumers

The canonical node producer calls the retained accounting builder only when the native accounting fence is active at its current DAA. It binds the literal executed job context, executed answer count and producer backend output root before creating explicit transaction carriage. The ordinary probe measures the actual larger outer transaction before choosing its fee. The common builder preserves existing inputs, outputs, fee and literal inner payload and re-signs all funding inputs over the actual outer transaction. It never silently deducts an envelope fee from a funding output. Other callers must provide sufficient fee; native fee rejection remains authoritative.

The explicit builder preserves plain, TFP1, HFP1 and TFO1 literal wrapper order. Outer FPA1 bytes participate in the actual transaction ID and native funding signature. For TFO1, the actual outer outputs remain the own-transaction funding outputs; inner payer consent terms have their separate signature context and do not authenticate a replacement outer answer. The builder receives the producer's root result; that result is not a computation proof or verifier approval.

The canonical collector retains additional accounting wire/signing leases through native submission. These leases cover only added accounting work, not preexisting transaction allocations. Typed preflight and native reservation precede newly decoded originals, answer wire allocation and signing workspace. Semantic refusal and native capacity refusal remain distinct.

Before every approving seat receipt/vertex path, the node reads acceptance data and bodies from the same guarded consensus session. The accepted block's DAA resolves the fence. The consumer binds the actual accepted transaction identity to the duty claim, job pin, roots and executor, then obtains its own backend context and output root over the literal transaction-carried answer. Legacy served-material FPA1 is not a substitute. Undecodable unrelated originals are skipped; capacity refusals propagate. Absence of the target carrier, a mismatched target, missing history, corrupt accepted indices or ambiguous target carriers withhold approval.

The submission planner decodes accounting originals through retained bounded preflight and refuses staged answer IDs that differ from the literal transaction before writing files. Existing served material keeps its separate replay format.

The native funded-original diagnostic recognizes FPA1 around TFO1 only at the native accounting fence. It retains actual validated transaction ID, input fee and funding outputs while reading literal inner terms. Its result remains Invalid, Missing or Resource; it adds no admitted result or installer authority.

## Focused controls

Actual node execution: **5/5 PASS**, 0.80 s. Binary `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspad_lib-5c1f30073b2241b4`, SHA-256 `45f5f850d88d91f0291e94511cbb023c7d2b39623dcdc1363bae19c723531d9c`; log `/tmp/misaka-rfc38-node-seat-tests-20261024.log`. This is a compiled factory-feature binary; unrelated factory controls are recorded separately.

Earlier metadata attempts found missing synchronous session getters (E0599) and a test prompt-token type mismatch (E0308). The reader now uses the same guarded session through `with_consensus_api_v1`; the fixture converts prompt IDs to the backend input type. The successful node binary includes both corrections.

Core accounting group execution on 2026-10-25: **10/10 PASS**, 0.10 s (new binding control plus nine existing controls). Binary `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspa_consensus_core-210c32867c0e9198`, SHA-256 `76a62f2b44259277d527bd9a06ecb5e67f6dee3101adb729286f8774bc2a587f`; log `/tmp/misaka-rfc-parallel-palw_fp_answer_accounting_v1-20261025.log`. The group includes the existing arming-refusal control.

The initial PQ test build failed with E0425 because the new control referenced an existing verifier in a sibling private test module. The test now explicitly uses that verifier with test-only parent visibility. This was a compile failure, not a passing execution; the subsequent successful rerun is recorded below. Log `/tmp/misaka-rfc38-pq-accounting-tests-20261025.log`. No production source changed for this correction.

The first PQ runtime rerun failed before the new accounting loop: a preexisting assertion compared independently signed plain/history payload bytes, although ML-DSA signing uses fresh hedged randomness. The control now compares version, exact commitment and prompt IDs and verifies both original claim signatures and both actual funding signatures. The new accounting-wrapper check still requires byte-exact preservation of its own input payload. Log `/tmp/misaka-rfc38-pq-accounting-tests-retry1-20261025.log`; the subsequent successful rerun is recorded below. No production behavior was changed.

Remaining focused runtime results on 2026-10-25 (all binaries under `/tmp/misaka-rfc06-12-build-20261024/debug/deps`):

| Scope | Result | Binary | SHA-256 | Log |
| --- | --- | --- | --- | --- |
| PQ retry 2 | 1/1 PASS, 0.26 s | `kaspa_pq_validator_core-71221f43ae005f6c` | `a3151570ab3686fb65907dfda739d8d91a45d4aa48faa5524a2d3906c8002d85` | `/tmp/misaka-rfc38-pq-accounting-tests-retry2-20261025.log` |
| Staging | 1/1 PASS, 0.00 s | `misaka_palw_fp_submit-ebc2082b5901f0f4` | `14514792009487d31af574c5c1241e3a09b67830105294e4140327a7668e18ca` | `/tmp/misaka-rfc38-staging-tests-20261025.log` |
| Native funded diagnostic | 1/1 PASS, 1.27 s | `kaspa_consensus-807ee71a89c08594` | `7d559cc58a0a0bfdeab6095a4dd5f57268d68ff4e04e26c98562619316381bff` | `/tmp/misaka-rfc38-funded-diagnostic-tests-20261025.log` |

The earlier PQ failures remain recorded above; passing reruns do not relabel those failed attempts. These focused controls are not campaign runs.

Per-control status:

- **PASS 1/1 (group 10/10)** Core `accounting_binding_codec_bounds_and_wrapper_order`: plain/TFP1/HFP1/TFO1 location, bounded carriage, nested/oversize rejection; framing alone does not admit a claim.
- **PASS 1/1** PQ `typed_original_public_fp_envelope_preserves_claim_bytes_and_signs_final_funding_payload`: actual plain/HFP1 producer wrapper, inactive-fence identity, real outer ML-DSA funding signature, old-payload signature refusal, unchanged outputs and outpoint, wrong-root and nested refusal.
- **PASS 5/5** Node `accounting_seat_`: actual BASE0 context/root binding, changed answer/job/root refusal, ambiguous same-claim carriers, missing or stale acceptance history, unrelated malformed original and old served-material separation. Transport fixtures do not establish native admission or verifier authorization.
- **PASS 1/1** Submit planner `accounting_transaction_plan_binds_staged_answer_before_writing`: matching literal answer and mismatched staged answer refusal before files.
- **PASS 1/1** Consensus `palw_funded_original_real_native_inputs_preserve_fallback_and_refuse_incomplete_admission`: actual Full UTXO input verification of FPA1/TFO1 outer signature, unchanged fee/output/state and Missing read-only diagnostic. Placeholder answer IDs are transport controls, not output-root/correctness evidence.

The final factory-feature node binary reran the same five seat controls on
2026-10-25: **5/5 PASS**, 0.72 s, not five additional distinct controls.
Its SHA-256 is `d8a7fa7264af8d1ba81c14ce79b021a350ae61128180085b0cd4f3674a91a603`;
log `/tmp/misaka-rfc38-node-seat-tests-final-20261025.log`. The binary path is the
same mutable cache path recorded above, and its prior bytes were replaced by
the fresh build.

Final metadata checks passed: `cargo check -p kaspad --lib --tests --bin misaka-rfc12-private-node --features private-integration-config`
(`/tmp/misaka-rfc-parallel-node-check-path-admission-final-20261025.log`) and
`cargo check -p misaka-palw-gateway --bins`
(`/tmp/misaka-rfc-parallel-gateway-check-final-20261025.log`). These confirm
compilation, including the rail caller of the retained staging decoder; they
do not establish live producer emission or acceptance.

## Remaining arming blockers and limitations

The arming validator still returns Err and all public accounting/full-activation presets remain None. No chain rules or public activation heights are changed.

The native seat duty and claim schema lack an exact winning carrier ID. Two accepted carriers for the same claim therefore fail closed rather than selecting by iteration order. An actual winning-carrier index is still required for unambiguous arming.

Only the canonical node producer now emits explicit carriage. Rail, SDK, worker and gateway emission callers do not yet have a complete authoritative native fence/context handshake. They must not infer activation from caller-supplied flags or use legacy served answers as accounting evidence. Staging understands new carriage, but that does not complete those emission paths. A live multi-input TFO1 producer caller and its end-to-end emission acceptance remain unverified.

The current commitment-price RPC does not take answer IDs. Answer-aware pricing and all-model calibrated answer/context/root computation costs remain unproven. Existing body-store allocations and backend execution workspace are not newly covered by the additional wire/signing leases.

Claim authentication still requires selected-verifier approval under its explicit honest-fraction assumption. Context/root identity, matching replay and an optimistic Final do not replace that approval. No model-level screening or cryptographic model-computation proof is introduced.
