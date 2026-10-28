# RFC-0035 A: signed funding preparation and separate transport

Status: **isolated first stage; funded original admission remains unavailable**. No new escrow state, production adoption, pre integration, arming or deployment.

Three new components prepare the selected A contract. `palw_court_original_escrow_v1` binds a new payer signature to the original identities, relative graph/profile identities, acceptance interval, material horizon, funding output index/amount and payer refund key. The final carrier txid is obtained from the actual transaction rather than self-referenced in its signed payload. Own-output facts reject a stale cached txid; the signed output must match an exact version0 payer P2PKH-MLDSA87 fallback. Unknown script versions and unrestricted scripts are refused.

`palw_tir_funded_original_envelope_v1` introduces separate TFO1 framing with the literal old TFP1 bytes preserved. It bounds signature/key/vector allocation and strict EOF within the existing TFP1 outer ceiling. It pairs output inspection and fee preparation to the identical carrier. The accepted transaction object extractor explicitly refuses this new framing independently of the existing history/TIR fences; it does not emit the embedded legacy FP. Ordinary accepted carrier UTXO/fee effects may remain, without creating a funded escrow.

`palw_court_carrier_fee_v1` uses populated input entries and real transaction outputs to derive a checked difference. Duplicate inputs, stale identity, missing entries, overflow, output excess, duplicate allocation identities and total fee over-allocation fail. The funding output is already subtracted as an output and cannot also supply fee credit. A native coordinator must still obtain entries from its real composed UTXO view, perform ordinary signature/script validation and assign all eligible burn/rent/fee duties through one budget. Recreating separate budgets would not provide global conservation.

These components return local preparation facts only. Native expected scope is a DTO, not evidence of canonical graph derivation. A caller that also invents the expected scope does not acquire original-admission, spend or rooted-funding authority. No new rooted row, spend bypass, paid flag, balance or fake completion capability is created. The valid funding tests use real MLDSA signatures; transport fixtures explicitly use invalid synthetic signatures to ensure structural decoding does not authenticate consent. Test amounts/profile hashes are fixture values, not prices, measurements or adopted production parameters.

## Verification

[Exact commands, source hashes and pinned executable metadata](funded-original-stage1-verification.json) record a fresh release build, copied executables and unchanged source hashes across verification. The source snapshot contains the changed Rust files, and the tracked patch records the build-phase base delta.

**29 unique selected passes, zero failures/ignored**, from the new executables:

| Selection | Passes | Raw log |
| --- | --- | --- |
| Checked fee budget | 2 | [fee](funded-original-stage1-fee.log) |
| Real payer signature and actual output preparation | 4 | [funding](funded-original-stage1-funding.log) |
| TFO1 framing, refusal and carrier pairing | 4 | [envelope](funded-original-stage1-envelope.log) |
| Existing TFP1 behavior | 5 | [legacy](funded-original-stage1-legacy-envelope.log) |
| t12 identity | 9 | [identity](funded-original-stage1-t12.log) |
| Existing activation behavior | 5 | [activation](funded-original-stage1-activation.log) |

[Release build](funded-original-stage1-build.log) and [non-test consensus library check](funded-original-stage1-consensus-check.log) both exit0. These are selected component/identity checks, not the entire core suite or live funded node tests. The earlier pruning result remains its separately pinned service campaign.

[Two-run native wire/inventory check](funded-original-stage1-native-wire-check.log) exits0: generated outputs are byte-identical to the committed outputs. No rooted schema or generated wire inventory changed.

[Document check](funded-original-stage1-docs-check.log) remains FAIL/exit1 with the same 5 imported missing references and 43 known baseline occurrences; no new authored missing reference or stale index was introduced. Baseline and imported raw assets remain unchanged.

## Remaining closure

Actual selected-chain UTXO validation, native escrow locks including same-block/mergeset protection, qualified third-party settlement without a future payer signature, once-only rooted accounting, actual payee/refund/burn/change outputs, and atomic original admission remain unimplemented. Complete canonical duty/role/dependency/resource/capital/fee graphs and physical profiles are still required before opening the new path. This stage does not complete any whole RFC-0026 T01–T15 scenario or establish model door, throughput, Panel profitability or heartbeat measurements.

## Later native address correction — 2026-10-22

A real native input-validator regression found that the initial candidate conflated the unkeyed payer identity with the domain-separated native P2PKH address. Its helper successes therefore did not establish actual fallback spendability. The [preserved failure and corrective verification](../g14-completion-2026-10-22/material-and-native-candidate.md) separate those identities and check the actual public-key-derived output/refund address. Earlier source snapshots, executables, command outputs and measurement values remain unchanged; this note does not upgrade their scope.
