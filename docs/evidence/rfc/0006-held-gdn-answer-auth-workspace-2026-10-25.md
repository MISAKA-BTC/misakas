# RFC-0006 held-GDN answer authentication workspace

Status: corrected Core 2 and actual Node 2 parent tests PASS; related controls also PASS. This is one local construction boundary, not RFC completion or public activation.

## Actual production connection

`palw_gdn_held_step_retained_v2` calls `note_answers_v1` before deciding whether an old demand times out. The real host tick also reads answers while its history observation is incomplete. Both use `answer_on_chain_v1`, which previously ran the raw disclosure validator before the retained-copy admission and discarded its semantic result through `.ok()`.

The reader now invokes `palw_held_da_state_chunk_auth_request_v2(...).check_v2()` on each matching borrowed carriage. Semantic invalidity continues scanning as before; local Resource returns immediately, so it cannot become a missing answer, default, honest case, signature or cursor advance. Only successful authentication proceeds to the existing independent retained-disclosure clone owner. Existing wire objects, signature roles, disclosure bytes and raw validator remain unchanged.

The additive typed request supports **StateChunk demands under the held v4/v5 maps** used by this GDN consumer. Other unit/map capability requests explicitly return local Resource, with no unreserved raw fallback. This is not general coverage of all held-DA units or historical maps. The raw API remains available with its existing behavior.

## Quote and lifetime

The request is constructed from borrowed source values and checked arithmetic, without computing a root or building a native layout:

- The profile's real canonical Borsh length measures its identity writer; the existing Borsh writer starts at 1024 bytes.
- The job CanonicalWriter starts at 512 and writes `u16 + u32 network length + eleven Hash64 + fixed32 seed + three u32 + network bytes`, or 754 fixed bytes plus actual network length.
- The existing fixed checkpoint-profile, step-leg, checkpoint-leg and execution-commitment writers are quoted from their actual fields and their 256-byte initial buffers. Binding authentication happens twice in the old checker; the quote conservatively sums the sequential roles.
- The native held-map Attention/GDN layer iterators are shared by layout construction and preflight. Actual layer counts quote typed `Vec<u16>` growth, including old/replacement capacity. The raw validator uses one layout at a time; the quote conservatively includes both A/G lists even at a checkpoint without recurrence.
- The checkpoint-leaf writer is `u16 + five Hash64 + three u32`. The chunk-leaf writer reads actual borrowed chunk bytes with its `Hash64 + three u32` prefix. Checkpoint Merkle leaf and slice-top writers are fixed typed spans. Opening membership folds borrow sibling slices and scalars; they do not need invented sibling/path copies.

The growable-writer envelope covers old and replacement buffers using checked next-power-of-two arithmetic; it is a conservative logical admission bound, not exact allocation, RSS or all-model cost. The workspace ticket lives around the **same raw validator, called once**, through its last hash/layout temporary. That ticket drops when the scalar authentication result returns. Persistent answer/output copies and their owners remain separate. This scope does not claim ownership of source inputs or retained semantic-error strings.

A subtle earlier allocation was in map dispatch itself: hybrid v1–v5 ID queries built formatted names. A minimal shared parts-hash helper now hashes the same domain and exact concatenated name parts without allocating a String. The String-returning canonical name APIs are unchanged. All five IDs are compared with the legacy formatted preimages in the new core control; this is data identity only.

## Prepared controls

Package `kaspa-consensus-core --lib`, filter `palw_held_da_v1::auth_workspace::tests` (two parents):

1. Existing real held-checkpoint fixture: valid/invalid chunk and anchor raw/typed semantic parity, source Borsh unchanged, five-generation map-ID canonical-byte parity, and actual Writer growth-boundary controls.
2. Isolated quota child: borrowed scalar authentication at zero and Q-1 returns Resource; Q authenticates and releases its temporary ticket before an independent full-Q probe. Unsupported unit capability refuses locally. The child is included in its parent count.

Package `kaspad --lib` with parent-selected private/EVM features, filter `palw_gdn_held_filer::tests::answer_auth_tests` (two parents):

1. Actual registered Q36 transition/convolution fixtures: foreign and invalid earlier carriages are skipped, a valid later response has identical raw disclosure bytes, and typed authentication succeeds.
2. Isolated actual late-demand step: source/case/disclosures are built before arming quota. A scalar-only Q authentication and release proves the target is authentication rather than retained copy. With zero/Q-1 available, the real step refuses typed Resource before any answer/cursor/deadline/signature/default change. After capacity release, the same case retries through the existing native court and actual signed action. Both transition and convolution are exercised; all resulting owners are dropped before a full-capacity probe. The child is not an additional parent test.

The small core fixture is attention-only; the actual Q36 GDN fixture includes recurrence. These are scoped mechanisms, not K3 or all model/cadence coverage. Existing answer-owner/full-held, core held-DA, checkpoint/state-map and map-ID tests are recommended related regressions because they share the unchanged raw validator and hash identities.

## Static verification and limits

New leaves are rustfmt formatted; existing files are parsed through rustfmt stdout with `skip_children=true` only. `git diff --check` and the offline documentation checker must pass before source freeze. No agent Cargo, deployment, live DB access or campaign was performed.

Formal K3 specification/manifest/weights/signed inputs remain uncreated; all-model cost and the fifteen campaign gates are not closed here. Prefix/replay/serialization input boundaries, generic host/history maps, all DA variants and other payout/court/aux consumers remain separately scoped. Activation None, normal bootstrap refusals, model tiers and claim-selected-verifier approval remain unchanged. No computation proof or model screening is introduced.

## First integrated runtime attempt — not accepted yet

Six-package build succeeded in 728.15 seconds, with the source/config identities unchanged during compilation. Record: `/tmp/misaka-rfc-gdn-mesh-red-source-20261025/record.json`; log `/tmp/misaka-rfc-gdn-mesh-red-build-20261025.log`. Six SHA256-pinned executables are recorded in `/tmp/misaka-rfc-gdn-mesh-red-pins-20261025.json`.

Core authentication controls: 2 PASS / 0 failed / 0 ignored. Node reader invalid-skip/raw-byte control: PASS. Node late-step parent: FAIL at the expected-Accuse assertion after capacity release; its completed assertions before that failure are not extra accepted tests. Logs: `/tmp/misaka-rfc-gdn-mesh-red-core-runtime-20261025.log` and `/tmp/misaka-rfc-gdn-mesh-red-node-runtime-20261025.log`.

Read-only diagnosis found that the convolution-forgery fixture still selected the first, honest Transition candidate. The real step may correctly move on to a ConvWindow and request its other units rather than accuse immediately. The intended correction is to select the actual first `ConvWindow { head: 0 }` before constructing the demand answers and quota experiment, as existing court controls do. No production verdict/guard change or weakened expected accusation is justified by this failure. That correction and its runtime acceptance are still pending.

## Initial verification and preserved fixture correction

The parent six-package build passed in **728.15 s**, with source identity matching and six immutable binary pins. This build duration is not a model-cost measurement. The pin inventory is `/tmp/misaka-rfc-gdn-mesh-red-pins-20261025.json`.

- Core filter: **2/2 PASS**, 0.05 s harness; its isolated child 1/1, 0.01 s is included. Log: `/tmp/misaka-rfc-gdn-mesh-red-core-runtime-20261025.log`. Core pin `/tmp/misaka-rfc-gdn-mesh-red-kaspa_consensus_core-20261025`, SHA256 `32d5cdff6289ab806e161eb4c235d769188a60769788d3e06335a88c86e6afae`.
- Node filter: **1/2 PASS**, 13.66 s harness; reader invalid-skip/byte parity passed, while late-step retry's immediate `Accuse` expectation failed. The failed child (7.47 s) is included. Log: `/tmp/misaka-rfc-gdn-mesh-red-node-runtime-20261025.log`. Node pin `/tmp/misaka-rfc-gdn-mesh-red-kaspad_lib-20261025`, SHA256 `c293b89e91944d7c2b9ef3ac1d1b66f163c75f990476d363ff7274f2b27e155e`.

Read-only diagnosis found that the conv-only forged world started at the default first Transition candidate, whose delta state is honest. The actual step can correctly advance to a ConvWindow and demand its different missing units; it is not obliged to accuse immediately from the Transition's answer set. The failure did not log the returned enum, so that enum is not retrospectively asserted as observed.

The correction follows the existing court-resource/conv-owner fixture: for the conv world, load its actual first `ConvWindow { head: 0 }` before building disclosures and the authentication request and before quota arming. Assert transition/conv candidate types explicitly. No production code, native court predicate, signatures, capacity settings or expected final accusation/release assertions are relaxed. Runtime of this correction remains **PENDING** and will be reported on its own final pin; the first failure remains preserved above.

## Final local verification — 2026-10-26

ConvWindow fixture correction passed without weakening accusation/default/signature controls. Earlier failures remain historical. The fixed six-package build matched its source manifest and six executables were pinned in persistent artifacts. [Full source/binary/log evidence: 53 distinct parents](0006-0008-gdn-auth-mesh-once-verification-2026-10-26.json). This does not close the whole RFC or campaign.
