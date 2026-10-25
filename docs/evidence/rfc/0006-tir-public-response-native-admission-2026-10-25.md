# RFC-0006 TIR public response native admission — 2026-10-25

Status: scoped compilation and runtime PASS. The separate finished-material loader/Node consumer vertical is recorded in [its continuation evidence](0029-finished-material-native-owner-continuation-2026-10-25.md). This support record supplies no campaign PASS.

`PalwTirPublicSourceV1` now has typed native response decode, response authentication and complete-ID-body construction APIs. Their errors preserve `NativeDecodeErrorV1::{Resource,Decode}`. Existing raw APIs use their original codec/allocation-error branches and original semantic authentication. No wire tag, field order, request policy, source identity, public activation, credit approval, or model-tier change is introduced.

## Decode before allocation

`decode_response_native_v2` first applies the existing Source request/frame byte bound, then an allocation-free bounded Borsh scan. It handles all seven variants: StepNode, StepLeaf, StepRun, RowsNode, TileNode, TraceTile and Ids. Each actual vector count is checked against its concrete format bound and the remaining frame before the destination is constructed. The scan counts `Hash64`, `u32`/`i32`, `PalwStepTileLeafV1` slots and nested leaf byte vectors by their native types. Scalar leaf coordinates are read through their actual Borsh type, with exhaustive field destructuring. The scanner rejects truncated/unknown/trailing fields without constructing a decoded response.

The native decoder then reserves the scanned heap before invoking the shared bounded parser. Its vectors use fallible exact reservation and reject an unexpected capacity. Raw parsing keeps its prior behavior, including the raw bounded-byte helper and raw StepRun allocation branch. Native physical allocation refusal uses actual `io::ErrorKind::OutOfMemory`; classification never inspects diagnostic text. A returned `NativeRetainedV1<Response>` drops its value before its lease.

## Authentication owner and workspace

`authenticate_response_native_v2` checks the existing request/vector/byte bounds before admission, reserves the exact response clone heap and the separately derived temporary authentication envelope, then invokes the existing `authenticate_with_rows_v1` once. The public three-argument `authenticate_response_owned_v2` is the identity-only twin of the existing public authentication method; it supplies no native rows authority. The Core loader may provide its existing private native rows anchor through the four-argument method.

The workspace follows actual native consumers:

- StepNode/RowsNode/TileNode: current frontier copy plus the native `ceil(n/2)` next-vector capacity. Later levels shrink.
- StepRun: the maximum sequential leaf hash Writer envelope or current range-fold nodes plus native `n/2+2` next capacity. Widths after the first cannot exceed this peak (one node stays one; two remain at most two).
- StepLeaf/StepRun leaf hash: the real version, context/class hashes, coordinate, value count, byte-length prefix and actual values. The Writer starts at 256 bytes. Two checked power-of-two envelopes conservatively cover old/new growth; this is a type-sized upper admission, not a minimal/exact allocator-cost claim.
- TraceTile: the actual `i32` lane-byte Writer. Row/path walks otherwise use stack hash state.

Authentication returns a retained authenticated unit carrying the cloned response heap. Temporary workspace retires after the native check/clone. Membership authentication deliberately preserves a genuinely committed malformed preimage for the subsequent semantic court, as the raw route does; it neither judges model correctness nor replaces selected-verifier approval. The existing raw helper's committed-malformed controls also invoke the new native decode/auth twins.

## Complete accepted IDs

`decode_complete_ids_body_native_v2` accepts the existing complete FullIds body: concatenated canonical Borsh Ids responses at offsets 0, 1024, and so on, with the final exact partial chunk. A first borrowed pass checks tags, counts, remaining bytes, one rows-root identity and exact EOF. It creates no chunk response or provisional ID Vec. One exact total-`u32` destination is admitted before construction. A second pass fills that destination; the existing typed complete-ID validator separately admits its ID-byte/context hash scratch and verifies the original trace root. The same destination moves into the returned retained authenticated IDs, without an assembler growth sequence or final deep clone. The caller's input body owner is separate and must remain live through this operation.

## Actual pinned tests

Core filter `palw_tir_public_source_v1::response_native_v2::tests` contains five parents:

1. All seven variants match the raw codec and existing original native membership authentication; IDs remain provisional until full-body EOF validation.
2. Malicious counts and every truncated response return Decode even with zero quota; valid nonempty responses return Resource. Actual I/O error variants, not text, control capacity classification.
3. Decode and authentication stage requests refuse at Q−1, succeed at Q, keep output charged and release temporary workspace; actual native semantic refusal releases all leases.
4. A 2050-ID, three-chunk body matches the raw canonical assembler and refuses truncation, changed root, noncanonical lengths, bad EOF and trailing material.
5. Actual complete-ID root scratch refuses at its typed peak Q−1 after destination admission, succeeds at Q, and leaves only the retained ID output charged. Existing validator observations distinguish this from an earlier parser/source refusal; the experiment uses real quota, not observations as a substitute.

All five parents passed on the immutable Core test binary `/tmp/misaka-rfc-material-authority-kaspa_consensus_core-retry2-20261025`, SHA-256 `32512a7cfb469038cf32fc7846123934786d96192400376387c64afad6fad11e`. The focused harness finished in 0.06s (2.39s wall time including isolated process startup); children are inclusive parts of their parents. Log: `/tmp/misaka-rfc-material-authority-response-20261025.log`. Machine result: `/tmp/misaka-rfc-material-authority-focused-results-20261025.json`.

The coordinated five-package retry2 build passed in 276.60s, with all 29 frozen identities matching and five immutable binaries saved. The source/artifact record is `/tmp/misaka-rfc-material-authority-wave-source-retry2-20261025/record.json`; build log `/tmp/misaka-rfc-material-authority-wave-build-retry2-20261025.log`. These times include shared compilation/startup and are not a model cost or throughput measurement.

Failure history of the broader material lane is preserved separately from these Source tests: the first build failed E0599 in `palw_tir_public_state_v1.rs` (a borrowed custody check called a method unavailable on that contract type), and retry1 failed two E0382 errors in test observations in `palw_tir_finished_material_owned_v2.rs` (consuming the request quote twice). Logs are `/tmp/misaka-rfc-material-authority-wave-build-20261025.log` and `/tmp/misaka-rfc-material-authority-wave-build-retry1-20261025.log`. The material lane corrected those sites before the passing retry2 build; the three Source-support Rust files required no compilation correction. Failed build logs are not runtime PASS.

The same coordinated snapshot also passed the separate finished-material 2/2 and Node sharing 2/2 focused controls, whose ownership/actual caller scope belongs to the linked continuation record. Those counts and this Source 5/5 are reported by their own filters, without counting children or overlapping regressions as new independent evidence. Related controls passed on the same immutable Core binary:

| Filter/scope | Actual result | Log |
| --- | --- | --- |
| Existing `palw_tir_public_source_v1::tests` (including genuinely committed malformed preimages; helper now exercises native twins) | 7/7 PASS, 0.03s harness | `/tmp/misaka-rfc-material-authority-source-regression-20261025.log` |
| `continuation_source_owned` | 3/3 PASS, 1.42s harness | `/tmp/misaka-rfc-material-authority-continuation-regression-20261025.log` |
| `complete_ids_owned` | 3/3 PASS, 0.16s harness | `/tmp/misaka-rfc-material-authority-ids-regression-20261025.log` |

Controller: `/tmp/misaka-rfc-material-authority-regression-results-20261025.json` (all 15 coordinated regression groups passed). These related filters support their stated mechanism scope; the 15 regression groups are not the 15 real campaign gates, and no campaign status is promoted.

This scope does not cover all payout/court/aux consumers, arbitrary replay/prefix work, transport/raw-frame creation, whole-node physical RSS, all-model cost, formal K3 artifacts, 100k/pruned-P2P campaign completion, or the 15 real campaign gates. Native source/data authentication is not a cryptographic proof of model computation.
