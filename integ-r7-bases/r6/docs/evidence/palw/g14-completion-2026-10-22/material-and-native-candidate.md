# Material reconstruction and native input candidate checkpoint

Status: selected verification passed **43 unique tests (core42 + native1)**. This is an isolated implementation checkpoint, not complete G14, funded original admission, T01–T15, arming, pre integration or deployment.

## Implemented boundary

The existing shared TIR material store now has a separate crate-private Constraint terminal builder. Its distinct signed intent binds network, claim/source scope, policy, immutable claim semantics, material identity, position/lane/witness and current filer. It reconstructs the decode view and initial Constraint court from retained original/frame/registered semantics and authenticated IDs/tile material, then uses existing primitive authentication and adjudication. Returned borrowed evidence reconstructs and checks again before exposing its local finding. It is not a rooted owned certificate, paid receipt or consensus credit. The numerical intent and old wire tags are unchanged.

The private replay uses existing maximum legal EOG/rendering chunks and explicit local call/material budgets. Those budgets do not reserve clone/scratch, provider lookup, signature, CPU/IO, carriage or retention and do not authenticate adopted prices or native profiles. Missing material and invalid material remain refusals. Complete-original and financial authority are unavailable.

Native input diagnostics use the same validated transaction, populated entries and calculated fee as the ordinary native UTXO validator. They check actual output/fee, payer consent, network/window, original signature and current producer/class/layout, then return explicit Missing for source construction metering, full grammar/profiles and the ordered original coordinator. There is no success/admitted variant. A generic `ValidatedTransaction` does not establish fresh Full-script provenance; selected-parent cached checks must not be relabelled as Full. Native graph/geometry/pledge expansion is not claimed by these diagnostics.

**Default production leaves the diagnostic disabled.** No legitimate shared-service/profile activation contract currently exists, so the ordinary processing wrapper supplies no diagnostic context and performs no added TFO decoding/signature work. The opt-in internal seam and real input-validator integration test do not constitute an enabled funded block path. Existing native TFO extraction refusal and ordinary payer fallback remain intact. Real escrow locks, third-party qualified spends and joint UTXO/rooted apply/undo/reorg are absent.

## Preserved first verification

[Initial source, patch, executable hashes and individual commands](material-native-first/verification.json) pin the release build and selections. Build exited0. The selected modules passed 23 unique tests: Tokenizer4, Constraint3, new material/intent2, t12 identity9 and activation5. The numeric-store filter matched **zero** tests and contributes no success count.

Two new fixture tests failed and remain preserved:

- The constrained original store path failed because its service binding lacked the required semantic dependency contract. The repair constructs a signed exact history dependency via production `frame_semantic_bundle_v1`, checks missing-contract refusal, then binds service. It does not weaken the dependency check.
- The cross-crate native fixture generator failed because changing the producer public key also changes job identity; its old header/context could not be reused. The repair regenerates the tiny execution after setting the real test key, checks original/header job identity equality, and retains actual `original_plan_v1` / `prepare_original_v1`, signature and carriage-roundtrip checks.

[Repair source identity](material-native-repair/verification.json) records that only those two new test files changed; production source and the other selected test bodies remained unchanged. Do not add repeated selections to the unique count.

The repaired constrained-original store test and native fixture generator both passed on the [separately pinned repaired core executable](material-native-repair/verification.json). Thus the selected core total is 25 unique successes; the two earlier failed executions remain failures in their original logs. No production core implementation changed for these fixture repairs.

The first native test build [failed with two private-method diagnostics](material-native-repair/native-build.log). The existing network-domain method was private to its processor child module. A one-line visibility-only repair exposes it to the surrounding virtual-processor module; its computation is unchanged. [Native repair source and build record](material-native-visibility-repair/native-verification.json) preserve that separate delta; no broader/public API or protocol value is introduced.

The [earlier source snapshot preparation error](material-native-snapshot-preparation-failure/preparation-error.json) occurred before any cargo/native invocation: the wrapper included newly copied `.rs` evidence in its Rust inventory. Its original wrapper/source are retained; the corrected inventory excludes `docs/**` while preserving added Rust snapshots separately.

## Native regression found a real address-binding bug

The [visibility-repaired native build succeeded but its first Full input check failed](material-native-visibility-repair/native-candidate-test.log) with `SignatureInvalid(VerifyError)`. The native fixture had used the unkeyed `mldsa87_key_id` as a P2PKH payload. Native `OpBlake2b512` uses `blake2b_512_address_payload` with the address domain instead. Existing helper-level funding checks also equated key identity with output/refund payload, so fixing the test input alone would still leave its intended fallback output unspendable.

The repair must preserve the unkeyed payer identity, existing wire fields and signature domain, while checking the funding output and refund destination against the native address derived from the actual payer public key. A foreign refund key or old unkeyed trap output must remain rejected. This is a correction to RFC-0035's already-required payer-recoverable fallback, not a new payout policy or an instruction to change global/legacy key IDs. Earlier helper successes did not exercise native script spending and do not establish that property. Their original logs remain unchanged.

## Address repair verification

The [address-repaired core selection](material-native-address-repair/verification.json) passed **42 unique tests**: original escrow5, transaction-bound preflight3, funded envelope4, legacy envelope5, Tokenizer4, Constraint3, material/intent2, constrained original builder1, native fixture generator1, t12 identity9 and activation5. Repeated earlier runs are not added. Key identity helpers, wire layout, signature domains, old tags and arming settings are unchanged.

`verify_signature_v1` now requires both the signed unkeyed key identity and signed native refund address to match the actual public key. `check_actual_output_v1` requires that public key and exact native P2PKH output. A correctly signed foreign refund or unkeyed trap refund is refused. The new core regression checks those cases explicitly. No global change to legacy identities or arbitrary authorized recipients was made; see the [limited financial audit](financial-key-id-address-audit.md).

The previously generated fixture is [preserved byte-for-byte](material-native-repair/generated-fixture-before-address-fix/preservation.json) before regenerating its public test producer payout using the correct native address. The repaired generator writes to a fresh pinned directory first; only its successful output becomes the normal codec fixture. Old generator logs and hashes are unchanged. The [final native source record](material-native-address-final/verification.json) adds a test-only branch-specific assertion so generic Invalid/stale-ID refusal cannot masquerade as funding-output rejection.

The [final native build and input-validator regression](material-native-address-final/verification.json) passed. The test uses real ML-DSA ordinary input signatures and native UTXO validation, obtains Missing for the incomplete funded original, then successfully spends the ordinary fallback with the payer key. Wrong signatures, missing entries, expired consent, changed current producer, malformed inner contract and the explicitly identified unkeyed output trap are refused at their appropriate layers. This is an input-validator integration fixture, not a mined funded-block/mergeset/escrow/reorg campaign. Production diagnostic activation remains disabled.

## Fixture and remaining scope

The Constraint store positive scenario derives a real TIR program/frame and immutable semantics; its original pledge setup is the existing verified-half core fixture, not mined original signature admission. Its semantic dependency is checked using the production signed construction path. The separate native fixture generator uses a public deterministic test-only ML-DSA key, normal serialized state/params/envelope, and actual original preparation before export. Unknown fee/capital profile identities in the native refusal scenario are intentionally unknown; they are not proposed operational values.

These checks do not provide a paid public F03 path, actual F02 supplied opportunity, complete native original authority, money installation or a real-model latency result. Tokenizer partition invariance is a nonempty fixture check, not a production general canonical Tokenizer builder. See the [Tokenizer next seam](tokenizer-canonical-builder-next.md), [native memory preflight](source-graph-native-preflight-next.md) and [real escrow seam](native-escrow-essential-next.md). The [separate payload experiment](../../../measurements/2026-10-22-devnet-payload-carrier.md) measures standard carriers, not these complete court contracts.
