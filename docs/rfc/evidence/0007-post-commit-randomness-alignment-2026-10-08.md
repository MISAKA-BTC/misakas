# Post-commit challenge alignment — 2026-10-08

> **2026-10-10 navigation:** RFC05 now retains only its VM withdrawal reason. The selected §§K.0–K.8 design is in [versioned-kernels.md](../../design/palw/versioned-kernels.md); references below describe the audited revision, not a retained VM appendix.

Status: documentation alignment and static validation scope only. Baseline: public main
`51c94d36e80f87b74b0d5e166872b3fe6bb30d7c`. Inputs: the user's 2026-10-08 Kernel/model conformance and
PALW-work beacon design notes, plus the existing RFC/ADR texts. No crypto proof, runtime implementation,
beacon generation, conformance run, consensus test or network activation is claimed here.

## Authority and document map

| Document | Resulting responsibility |
| --- | --- |
| [RFC07 Part VI](../0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol) | Sole PostCommitChallengePolicyV1, PALW Work Beacon qualification/order/accumulator/lock, seed/sampling, per-message GKR timing, retry/reorg/scarcity and activation gates |
| [RFC02 §II.13](../0002-palw-tensor-ir.md) | Immutable class/plan/Kernel/challenge-policy binding before randomness; actual beacon/transcript remains later evidence |
| [RFC05 §§K.2/K.4](../0005-palw-ml-vm.md) | Approved checker/challenge/soundness tuple; Kernel conformance after semantic/constraint/court/material/resource completeness |
| [RFC11 §17](../0011-permissionless-model-and-long-context-onboarding.md) | Static Admission → Beacon Conformance → Active Eligibility; RegisteredDormant/pending distinct from ActiveRewardable |
| [RFC13 §9](../0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) | Commitment and beacon-conformance evidence, independent scoped results, source/transcript reconstruction, asynchronous resume and invalidation |
| [RFC14 §3.4](../0014-panel-independent-fraud-prosecution.md) | PalwDisputePlan policy binding and G14 fresh-outsider seed/query/transcript reconstruction through exact court/DA default |
| [RFC15 §1.1](../0015-panel-free-permissionless-verification.md) | Added post-commit completeness prerequisite; Panel=0 still deferred until every RFC14/RFC15 gate and explicit release |
| [RFC03](../0003-palw-generative-model-classes.md), [RFC04](../0004-palw-model-improvement.md), [RFC06](../0006-palw-layer-sharded-panels.md), [RFC09](../0009-palw-remote-miner.md), [RFC10](../0010-permissionless-palw-panel-and-claim-completion.md), [RFC12](../0012-palw-only-consensus-and-native-evm-settlement.md) | Generation/hold-out/Panel/ticket/remote-state/settlement contracts remain distinct; no alternate new verification seed or BFT/DNS authority |
| [RFC08](../0008-palw-claim-backed-consensus-blocks.md) and [EXEC spec](../../design/palw/rfc-0008-implementation-spec.md) | Policy per committed root/slice statement; root-open seed not reused for later freely chosen statements; zero EXEC weight/DAA and root-level work settlement |
| ADR0044/0074 | Legacy ticket/Panel source rules do not qualify a cheap floor/unadmitted header for the new challenge route |
| ADR0169/0171/0172/0173 and indexes | Shared policy, Kernel/model lifecycle, G14 replay and liveness decisions linked without changing old runtime/identities |

## Decisions preserved

Beacon randomness selects checks of a fixed statement, not semantic authorization. Unknown operations and missing
constraint/court/public-material/resource support remain ineligible. Independently validated existing source profiles
are needed; the consuming candidate/claim cannot self-seed or participate in a validity cycle. No fallback uses
heartbeat/BASE-0/EXEC hashes, local RNG, committee/threshold signatures or DNS-final authority.

Simple hashing of k Final commitments is not declared unbiased: source unpredictability, free rewrapping,
precomputation, paid-sample cost, adaptive last-contributor, withholding/collusion, retry/off-chain grinding and
canonical reorg/settlement analysis remain release blockers. Header/provenance wrappers stay outside seed-bearing
bytes. BeaconLocked is branch-relative, not a reorg veto. GKR still requires each message before its own staged
challenge or the separately reviewed transcript-bound Fiat–Shamir transform.

Same-seed watchers do not create independent repetitions. Conformance reports a checked scope/fault-model epsilon;
randomized tests prove neither unknown semantics nor Kernel soundness. Probabilistic mismatch is not conviction:
authenticated bounded localization and exact court/appropriate DA outcome remain necessary. Valid exact fraud proof
filing does not require a new beacon. Public evidence and liability survive the applicable retention/recovery rules.

Source scarcity delays onboarding/checks under their explicit deadlines, without changing current main heartbeat,
BASE-0, chain cadence, REAL, EXEC_TX or transaction liveness. Revised RFC8's former algo-11 design remains deleted;
its [historical test record](../../design/palw/rfc-0008-v0-test-record.md) retains its original results and limitations.

## Validation and limits

The documentation pass checks whitespace/diff integrity, newly added local links and section anchors, uniqueness
of the new protocol/seed definition, referenced policy authority, Markdown fences, documentation-only scope and
byte-preservation of the RFC8 historical S8 test record. These checks do not validate cryptographic soundness or
implement the proposed protocol. Existing historical source/fidelity/conformance measurements are unchanged.

Static checks passed for 24 Markdown documents and 45 newly added local file/section links: whitespace integrity,
fence parity, one canonical policy descriptor/seed formula, shared RFC02–15 authority references and byte-identical
RFC8 historical S8 record. No non-document files changed. This is document validation, not a consensus/cryptographic
test pass; real protocol tests and all §VI.8 gates are still pending.

No k, D, delay, hash/codec/tag, sampler, Fiat–Shamir transform, resource/retention budget or activation height is
selected by this revision. All RFC07 Part VI implementation/source/security gates and all new onboarding/G14 gates
remain open. The release must pin those values, implement and independently verify them, run real positive/adversarial
conformance and court paths, and show unchanged liveness under scarcity/flood before activation.
