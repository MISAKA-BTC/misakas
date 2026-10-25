# RFC-0006 Preparation/current-frame Source validation transients — 2026-10-25

Status: coordinated build and dedicated runtime PASS; see final verification below.

The actual Preparation/current-frame Source validator constructed `custody.liable_signers_v1()` as a temporary BTreeSet before the native Source reservation. It also checked native work receipts before Source construction and again against the resulting Source: `Preparation::check_v1` hashes aggregate and Ready-actor receipts, while `Owned::check_v1` hashes its aggregate receipt and, for an active demand, rederives its Source demand ID. Those raw helpers use temporary Borsh byte writers. The earlier Source constructor reservation starts later and does not admit these earlier validation allocations.

The Preparation subset now uses the same borrowed producer-or-original-grant membership predicate as acquisition. It preserves membership in the existing contract's liable-signers set without copying that set. It does not add drawn-seat credit, model review, grants, or authority.

The shared `public_state::hash` helper now streams the same Borsh byte sequence directly into the same keyed Blake2b state. This removes its descriptor/allocation/header identity Vec writers, including the ones reached by `row.key_v1`/`check_structure_v1` and `check_source`, while preserving raw return types, hash domains, field bytes and serialization error text. Source hashes keep their prior meaning as data identity, not model correctness.

The private raw/retained Source abstraction gains a validation-workspace hook. Raw Source APIs buy no new capacity and retain their String contract. Retained Preparation and acquisition APIs quote the actual borrowed aggregate/actor last-receipt serialized spans and the actual active demand tuple (Source ID, native demand scope and request). All such hashes are sequential: one checked old/new Borsh growth envelope covers their maximum span. Borsh starts at 1024 bytes; two checked power-of-two envelopes conservatively cover growth. If no receipt or active demand exists, this request is zero. This is a typed upper admission for these specific writers, not an encoded-byte/native-heap ratio or an exact RSS/minimal-cost claim.

The validation lease is acquired before `row.check_structure_v1` and lives through the final validation borrow of the independently retained Source. No validation buffer escapes. On returning a Source, its own constructor ticket remains; the completed validation scratch retires. On local capacity or semantic refusal, all acquired leases retire. Span/representability arithmetic remains typed Resource and is never classified by diagnostic text.

## Prepared actual-source controls — not yet run

New Core filters `source_validation_` and `source_identity_writer_borsh_layout_` contain three parents (one isolated child is inclusive):

- Streaming identity hashes match the prior keyed hash over canonical Borsh bytes for actual accepted Preparation/acquisition rows and the real header, plus a bulk byte-vector span. The serializer's refusal text is unchanged. Borrowed liable-membership matches the prior set over the fixture's producer, drawn seats and outsider.
- A real signed Capture receipt and a real signed pending Owner demand supply nonzero validation requests. The actual native getters refuse at validation Q−1 before Source construction; Q admits the validation stage but still refuses the separately reserved Source. Their combined peak admits the getter. A callback at the real final validation borrow verifies the real quota is occupied by both tickets; after returning, only Source remains charged. Retry, raw Source identity parity, unchanged rooted state and semantic error release are required.

- A separate native Source-construction writer correction now quotes `2 * next_power_of_two(max(serialized_identity_span, 1024))`. The prior `2 * span` could miss old/new buffers when a Borsh writer crosses a growth boundary: span 1030 requests only 2060 bytes although a 1024→2048 transition has a 3072-byte old/new envelope. A real layout's actual Borsh write sequence crosses that boundary; its output matches canonical Borsh bytes and the corrected quote covers the transition. The expanded layout is a standalone writer-geometry input, not registration or acceptance of a different model/class. Boundary/overflow controls also distinguish 1024/1025 and 2048/2049. This is a writer bound; no decoded-heap ratio or allocator/RSS cost is inferred.

The existing continuation Source test's exact-capacity control now includes each actual row's validation workspace. Its later caller borrow observes the separately retired validation workspace while proving the Source owner is still live. It does not silently relax the Source quote or replace a target-stage refusal with an earlier one.

Runtime, compilation, existing continuation regression and related material/Node controls remain PENDING. Static parsing and diff checks are not runtime acceptance.

## Explicit remaining boundary

The legacy fallback still uses `staged_original_job_v1` to decode its stored original commitment and can rehydrate legacy work quotes. This patch does not close those input/analysis costs or claim all fallback preparation costs. Source constructor program/layout/StepSpace admission, finished-body reconstruction, response decode/authentication and their retained outputs remain separate owners. Error-diagnostic allocation, physical allocator/RSS, all-model costs, formal K3 artifacts and real campaign gates remain outside this bounded writer/borrowed-set change. No Params, wire, activation, model tiers, claim approval or settlement policy changes are made.

## Final coordinated verification

Source three dedicated parents and related continuation/public-row/response/material checks PASS. Six-package retry1 no-run PASS in 186.13 seconds with 40 identities matched. Initial missing test import failure is preserved. Earlier PENDING statements describe the preparation checkpoint and are superseded by this terminal result. [Pinned executables, exact counts, logs and scope](0006-0012-source-cone-replay-logs-verification-2026-10-25.md). No campaign status is promoted.
