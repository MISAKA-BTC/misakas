# 原文の状態宣言台帳 — 204ファイル

**これは元snapshotの記載をそのまま取り出した台帳です。現在のruntime実装・試験・有効化状況の再監査ではありません。** 冒頭のStatusが古いまま後続改定を含む文書もあります。宣言だけで最終採択を自動判定せず、本文の判断範囲・後続改定・対象networkを照合してください。

明示的なStatus（または冒頭Decision: Accepted等）と、その行範囲を保存しています。定型宣言がない文書の状態は推測せず「定型宣言なし」と記録します。採択状態を含む見出し一覧は`source-manifest.json`の`decision_headings`にも残しています。

<a id="source-0001-network-isolation"></a>

## 0001-network-isolation.md

[原文](../adr/0001-network-isolation.md) / [新しい主担当](0001-network-identity-and-fingerprint.md)

原文 L12–L14

~~~~~~~~text
Status: Accepted (Phase 1)
Date: 2026-05-28
Supersedes: —
~~~~~~~~

判断・改定等を含む見出し（抽出）: L42: Decision

<a id="source-0002-mldsa65-p2pkh"></a>

## 0002-mldsa65-p2pkh.md

[原文](../adr/0002-mldsa65-p2pkh.md) / [新しい主担当](0002-post-quantum-signatures-and-interfaces.md)

原文 L12–L15

~~~~~~~~text
Status: Accepted (Phase 1). **Signature scheme superseded by [ADR-0019](0019-mldsa87-migration.md)** — ML-DSA-65 → **ML-DSA-87**, full PQ-only. The P2PKH structure and acceptance criteria here still hold; what changed: the scheme name/sizes (pk 2592 / sig 4627), the tx context `b"kaspa-pq-v1/tx/mldsa87"`, and the address payload (64-byte BLAKE2b-512). Source identifiers still read `MlDsa65` pending a cosmetic rename (design doc §J / ADR-0019 P8, deferred).
Date: 2026-05-28
Supersedes: —
Superseded-by: ADR-0019 (signature scheme)
~~~~~~~~

判断・改定等を含む見出し（抽出）: L38: Decision

<a id="source-0003-lthash-utxo-accumulator"></a>

## 0003-lthash-utxo-accumulator.md

[原文](../adr/0003-lthash-utxo-accumulator.md) / [新しい主担当](0003-utxo-accumulator-and-commitment.md)

原文 L12–L14

~~~~~~~~text
Status: Accepted (Phase 1); REVISED (audit QM-1) — migrated to LtHash32_1024.
Date: 2026-05-28 (rev. 2026-06-03)
Supersedes: —
~~~~~~~~

判断・改定等を含む見出し（抽出）: L59: Decision

<a id="source-0004-utxo-commitment64"></a>

## 0004-utxo-commitment64.md

[原文](../adr/0004-utxo-commitment64.md) / [新しい主担当](0003-utxo-accumulator-and-commitment.md)

原文 L12–L14

~~~~~~~~text
Status: Accepted (Phase 1 freeze; PR-7.6 introduced the type + finalize path)
Date: 2026-05-28
Supersedes: —
~~~~~~~~

判断・改定等を含む見出し（抽出）: L69: Decision

<a id="source-0005-mass-policy"></a>

## 0005-mass-policy.md

[原文](../adr/0005-mass-policy.md) / [新しい主担当](0004-mass-carriage-and-transaction-relay.md)

原文 L12–L15

~~~~~~~~text
Status: Accepted — **PR-19-S7 (Phase 7) recalibrated `mass_per_sig_op = 10_000`
for ML-DSA-87** (Phase 1 froze the shape; the earlier ML-DSA-65 PoC set 6000)
Date: 2026-05-28 (rev 2026-05-31)
Supersedes: —
~~~~~~~~

判断・改定等を含む見出し（抽出）: L17: PR-19-S7 (Phase 7) recalibration — ML-DSA-87 (supersedes the ML-DSA-65 result below) / L58: (historical) Phase 6 calibration result — ML-DSA-65 PoC, superseded by the above / L132: Decision

<a id="source-0006-rpc-wasm-sdk-types"></a>

## 0006-rpc-wasm-sdk-types.md

[原文](../adr/0006-rpc-wasm-sdk-types.md) / [新しい主担当](0002-post-quantum-signatures-and-interfaces.md)

原文 L12–L15

~~~~~~~~text
Status: Accepted (Phase 7 scope freeze)
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0002](0002-mldsa65-p2pkh.md), [ADR-0003](0003-lthash-utxo-accumulator.md), [ADR-0004](0004-utxo-commitment64.md)
~~~~~~~~

判断・改定等を含む見出し（抽出）: L43: Decision

<a id="source-0007-layered-pow"></a>

## 0007-layered-pow.md

[原文](../adr/0007-layered-pow.md) / [新しい主担当](0005-palw-consensus-architecture.md)

原文 L12–L13

~~~~~~~~text
Status: Accepted (Phase 1 design freeze; PR-8.1 — PR-8.3 land the foundations). **Still governing;
not contradicted by PALW.**
~~~~~~~~

原文 L261–L261

~~~~~~~~text
**Status: planned; NOT required for the Phase-1 single-algo launch.**
~~~~~~~~

判断・改定等を含む見出し（抽出）: L60: Decision / L307: Phase 2 — Argon2id (algo_id = 2): LANDED, then SUPERSEDED / L336: The trade-off (explicit and accepted)

<a id="source-0008-hash64-consensus-identity"></a>

## 0008-hash64-consensus-identity.md

[原文](../adr/0008-hash64-consensus-identity.md) / [新しい主担当](0001-network-identity-and-fingerprint.md)

原文 L12–L20

~~~~~~~~text
Status: Accepted (Phase 9 design freeze; PR-9.1 — PR-9.3 land the foundations)
Date: 2026-05-28
Supersedes: in part, [ADR-0004 §"Scope"](0004-utxo-commitment64.md)
            (UtxoCommitment64 was carved out as the only 64-byte
            field; this ADR widens the scope to all consensus
            identity).
Extends: [ADR-0007](0007-layered-pow.md) (the Layer 0 finalizer is
         the prototypical Hash64 producer; this ADR generalises that
         choice across the identity surface).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L46: Decision / L244: Implementation order (revised 9-phase plan)

<a id="source-0009-dns-probabilistic-finality"></a>

## 0009-dns-probabilistic-finality.md

[原文](../adr/0009-dns-probabilistic-finality.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0010-validator-node-architecture"></a>

## 0010-validator-node-architecture.md

[原文](../adr/0010-validator-node-architecture.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0011-validator-deployment-and-equivocation-safety"></a>

## 0011-validator-deployment-and-equivocation-safety.md

[原文](../adr/0011-validator-deployment-and-equivocation-safety.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0012-mainnet-validator-sortition-commit-reveal"></a>

## 0012-mainnet-validator-sortition-commit-reveal.md

[原文](../adr/0012-mainnet-validator-sortition-commit-reveal.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** Superseded — commit-reveal 案は 2026-05-30 に ADR-0017 で撤回。
~~~~~~~~

<a id="source-0013-validator-reward-distribution"></a>

## 0013-validator-reward-distribution.md

[原文](../adr/0013-validator-reward-distribution.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0014-coordinated-failover-protocol"></a>

## 0014-coordinated-failover-protocol.md

[原文](../adr/0014-coordinated-failover-protocol.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0015-remote-signer-hsm-protocol"></a>

## 0015-remote-signer-hsm-protocol.md

[原文](../adr/0015-remote-signer-hsm-protocol.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0016-stake-locked-bond-utxos"></a>

## 0016-stake-locked-bond-utxos.md

[原文](../adr/0016-stake-locked-bond-utxos.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0017-all-active-staker-attestation"></a>

## 0017-all-active-staker-attestation.md

[原文](../adr/0017-all-active-staker-attestation.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0018-quality-gated-stakescore-inclusion-economics"></a>

## 0018-quality-gated-stakescore-inclusion-economics.md

[原文](../adr/0018-quality-gated-stakescore-inclusion-economics.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0019-mldsa87-migration"></a>

## 0019-mldsa87-migration.md

[原文](../adr/0019-mldsa87-migration.md) / [新しい主担当](0002-post-quantum-signatures-and-interfaces.md)

原文 L12–L13

~~~~~~~~text
## Status
Accepted (2026-05-30); **Revised 1.1 — 2026-05-31** (scope expanded to PQ-only completion); **Revised 1.2 — 2026-06-01** (md2 alignment: identifier rename, v2 contexts, keyed address payload, 16_384 caps).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L12: Status / L29: Revision 1.2 — md2 (PQ-only design v2.0) alignment (2026-06-01) / L43: Revision 1.1 — PQ-only completion (2026-05-31) / L47: Locked decisions (this revision) / L52: Revised from 1.0 / L111: Decision

<a id="source-0020-selected-parent-evm-lane"></a>

## 0020-selected-parent-evm-lane.md

[原文](../adr/0020-selected-parent-evm-lane.md) / [新しい主担当](0007-evm-exec-lane-and-anchors.md)

原文 L14–L24

~~~~~~~~text
## Status
Accepted & implemented through design **v0.4 (mergeset delayed acceptance)**; **ACTIVATED ON
TESTNET 2026-06-11**. All build phases are on `pr-19-s5f-…`: P0–P3 (types/executor/state), M10
(acceptance executor + hot path), P4 (UTXO↔EVM bridge: deposit-lock → claim → credit → F002
withdraw → synthetic UTXO), §15 (template + wire), §16 (EVM mempool + RPC + indexes), §14 (EVM-tx
P2P relay, protocol 100→101 back-compat), plus the N0 optimization pass (O1 sender cache, O2
parallel admission, O3 Evm reuse, O9 lane KPIs). Proven live first on a 3-node devnet-EVM mesh,
then on testnet: `TESTNET_PARAMS.evm_activation_daa_score = 0` (genesis-active) via a 3-host
re-genesis cutover — the testnet genesis hash is unchanged (`cf4c48fe…`), and post-activation
headers carry `EVM_HEADER_VERSION` (v2), so pre-EVM nodes and the old chain are version-isolated
in both directions. Devnet is likewise genesis-active; **mainnet/simnet stay inert (`u64::MAX`)**.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L14: Status / L62: Decision — Selected-Parent EVM Lane

<a id="source-0021-palw-llm-pow"></a>

## 0021-palw-llm-pow.md

[原文](../adr/0021-palw-llm-pow.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** Superseded — 旧 algo 4/5 の報酬・PoW 有効化方針は撤回。
~~~~~~~~

<a id="source-0022-pruned-ibd-evm-overlay-snapshot"></a>

## 0022-pruned-ibd-evm-overlay-snapshot.md

[原文](../adr/0022-pruned-ibd-evm-overlay-snapshot.md) / [新しい主担当](0012-state-root-pruning-and-snapshots.md)

原文 L12–L14

~~~~~~~~text
Status: **Accepted (implementing)** — 2026-06-15
Supersedes: none. Depends on: ADR-0020 (Selected-Parent EVM Lane), ADR-0018 (PoS-v2
economics), DNS v3 Canonical Lagged Anchor, ADR-0007 Phase 3 (BLAKE2b-SHA3 PoW).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L56: 2. Decision

<a id="source-0023-base-three-lane-execution"></a>

## 0023-base-three-lane-execution.md

[原文](../adr/0023-base-three-lane-execution.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 不採用（将来の実装対象外）。
~~~~~~~~

<a id="source-0024-verified-llm-token-weighted-bft"></a>

## 0024-verified-llm-token-weighted-bft.md

[原文](../adr/0024-verified-llm-token-weighted-bft.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** Retired — VLT validator 投票重み・compute overlay は廃止済み。
~~~~~~~~

<a id="source-0025-chain-participation-and-ibd-candidate-selection"></a>

## 0025-chain-participation-and-ibd-candidate-selection.md

[原文](../adr/0025-chain-participation-and-ibd-candidate-selection.md) / [新しい主担当](0006-fork-choice-participation-and-settlement.md)

原文 L12–L14

~~~~~~~~text
**Status:** proposed
**Date:** 2026-08-09
**Supersedes nothing. Constrains:** IBD source selection, the sync-status RPCs, every signing path.
~~~~~~~~

<a id="source-0026-current-palw-testnet11-architecture"></a>

## 0026-current-palw-testnet11-architecture.md

[原文](../adr/0026-current-palw-testnet11-architecture.md) / [新しい主担当](0005-palw-consensus-architecture.md)

原文 L15–L15

~~~~~~~~text
Status: Accepted
~~~~~~~~

判断・改定等を含む見出し（抽出）: L17: Decision / L57: Mission alignment amendment — 2026-10-07

<a id="source-0026-palw-v2-runtime-separated-verification"></a>

## 0026-palw-v2-runtime-separated-verification.md

[原文](../adr/0026-palw-v2-runtime-separated-verification.md) / [新しい主担当](0005-palw-consensus-architecture.md)

原文 L15–L33

~~~~~~~~text
Status: **Accepted (architecture). Restored in full 2026-08-26; PROMOTED by ADR-0038 from credit
machinery to L1 machinery.** ADR-0051 walked this ADR's thesis back for one capped family — accepting
a tolerant proof model by name — and [ADR-0053](0053-palw-one-execution-family.md) withdrew that
family, so the exception is gone: take Ambient's architecture, refuse its proof model, build
exact-within-pinned-class, everywhere. The operating envelope as first written — devnet, shadow
mode, consensus-visible zero-credit only — was the envelope of the *credit-overlay* lineage, where
PALW rode beside a hash-produced block. On the V2 lineage
[ADR-0038](0038-palw-is-the-consensus-work.md) inverts the layering and
[ADR-0039](0039-palw-only-block-production.md) removes the hash path, so this ADR's verification
architecture is what *produces* blocks rather than what credits them; its activation is governed by
[ADR-0042](0042-palw-mainnet-candidate-ruleset.md)'s one atomic ruleset, not by the §12 ladder
alone. Every activation gate in the v2 design §12 still stands; this ADR moves none of them.
Date: 2026-08-15
Relates to: ADR-0007 (layered PoW), ADR-0021 (superseded PALW PoW record — kept as history),
ADR-0024 (verified-LLM token-weighted BFT),
[`palw-full-logits-trace-v2-design.md`](../design/palw/execution/palw-full-logits-trace-v2-design.md) (operative
safety model), [`ambient-pol-binary-audit-2026-08-15.md`](../audit/palw/ambient-pol-binary-audit-2026-08-15.md)
(the evidence this ADR rests on), the detailed / VPS-canonical-worker / secure-OTA design set,
`consensus/core/src/palw_v2.rs` (frozen preimages).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L98: Decision / L372: Mission alignment amendment — 2026-10-07

<a id="source-0027-palw-slash-unilateral-fraud-proofs"></a>

## 0027-palw-slash-unilateral-fraud-proofs.md

[原文](../adr/0027-palw-slash-unilateral-fraud-proofs.md) / [新しい主担当](0038-g14-public-prosecution-and-completeness.md)

原文 L15–L29

~~~~~~~~text
Status: **Accepted (architecture); PROMOTED by ADR-0038 from credit machinery to L1 machinery.**
Operating envelope as written: devnet / shadow / zero-credit. No slashing is enabled by this ADR; it
fixes what a slash may ever rest on — and that rule is unchanged by the promotion. What changed is
what a slash *bounds*: on the V2 lineage block production is bonded
([ADR-0039](0039-palw-only-block-production.md) Decision 6), so the aggregate-leverage half of
§4's `max_leverage ≤ 1.0` is enforced per bond by
[ADR-0042](0042-palw-mainnet-candidate-ruleset.md) Decision 6 (`reserved_exposure`), not by a
per-job credit price.
Date: 2026-08-15
Relates to: ADR-0026 (v2 verification architecture — amended here in §3/§4/§5),
[`misaka-palw-slash-protocol-design-v0.1.md`](../design/palw/verification/misaka-palw-slash-protocol-design-v0.1.md)
(the input specification this ADR adopts and amends),
[`palw-full-logits-trace-v2-design.md`](../design/palw/execution/palw-full-logits-trace-v2-design.md) (§10 economics,
§12 gates), `consensus/core/src/vlt.rs` (`ComputeFraudKind` — the existing enforcement of the same
rule at the VLT layer).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L81: Decision / L163: 4. Amendments to the v0.1 specification / L292: Mission alignment amendment — 2026-10-07 / L302: Bond予算・総影響保存の改定 — 2026-10-10

<a id="source-0028-palw-challenge-sampling-protocol"></a>

## 0028-palw-challenge-sampling-protocol.md

[原文](../adr/0028-palw-challenge-sampling-protocol.md) / [新しい主担当](0032-sampling-beacon-and-receipts.md)

原文 L24–L42

~~~~~~~~text
Status: **Accepted (architecture), with the §4e remedy set SUPERSEDED on the V2 lineage.**
Activates nothing; devnet / shadow / zero-credit envelope
unchanged. This ADR fixes the scheduling fabric that ADR-0027 left open: who re-executes which
job, when duties and windows open and close, what the randomness binds — and what it is
forbidden to decide. Promoted from Proposed on 2026-08-16 after a numeric review against the
real network parameters: the first draft's 48 h challenge window exceeded both pruning horizons
(30 h at 0.1 bps, 38 h on the 120 s net) — windows are now pruning-constrained, they derive a
credited-job ceiling (§3), and the funding mechanics are concrete (§4a–4e).
Date: 2026-08-16
Relates to: ADR-0027 (premises P1–P3, the refutation model this ADR schedules work for),
ADR-0026 §4/§5 (the PRF-positions flow demoted there is given its surviving role here),
[`misaka-palw-slash-protocol-design-v0.1.md`](../design/palw/verification/misaka-palw-slash-protocol-design-v0.1.md)
(§9 dynamic inputs, §10 DA, §17 objective offenses, §18 economics, §23 deadlines),
[`palw-full-logits-trace-v2-design.md`](../design/palw/execution/palw-full-logits-trace-v2-design.md) (§10 economics,
§12 gates, §13 staging, §15 prohibited claims),
[`palw-legs-capture-measurement-2026-08-15.md`](../evidence/palw/measurements/palw-legs-capture-measurement-2026-08-15.md)
(its §6 is the opening seam this ADR's §5 consumes), `consensus/core/src/vlt.rs` (`select_verifiers` — the assignment
shape adopted in §2), ADR-0012 (superseded commit-reveal sortition — deliberately **not**
revived; see "What this ADR deliberately does not decide").
~~~~~~~~

判断・改定等を含む見出し（抽出）: L72: Decision / L419: Mission alignment amendment — 2026-10-07 / L429: Bond予算・総影響保存の改定 — 2026-10-10

<a id="source-0029-palw-chain-carriage"></a>

## 0029-palw-chain-carriage.md

[原文](../adr/0029-palw-chain-carriage.md) / [新しい主担当](0004-mass-carriage-and-transaction-relay.md)

原文 L15–L27

~~~~~~~~text
Status: **Proposed (draft for review).** Activates nothing. This ADR decides how ADR-0028's
objects — job commitments, attestations, opening calls and answers, refutations — become
on-chain facts, in two stages whose first requires **zero node changes** and realizes the
Stage-0 telemetry (`P_check`, no-show, inclusion latency) as measurements of real chain events.
Date: 2026-08-16
Relates to: ADR-0028 (the scheduling fabric this carries; its Consequences named this gap),
ADR-0027 (evidence objects and DA rules), ADR-0026 §12 (the gate artifacts Stage 0 fills),
ADR-0009/0016/0017 (the DNS overlay carriage discipline this reuses),
`consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs`
(`check_transaction_subnetwork` — the admission gate), `consensus/core/src/dns_finality.rs`
(`dns_tx_kind`, the stateless validators), `consensus/src/pipeline/virtual_processor/processor.rs`
(the capability store's accept/revert/backfill walk — the Stage-1 template),
`consensus/core/src/palw_legs.rs` / `palw_slash.rs` / `palw_v2.rs` (the wire bodies, all frozen).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L68: Decision / L269: Mission alignment amendment — 2026-10-07

<a id="source-0030-palw-step-function-shape-profile"></a>

## 0030-palw-step-function-shape-profile.md

[原文](../adr/0030-palw-step-function-shape-profile.md) / [新しい主担当](0013-tir-arithmetic-and-determinism.md)

原文 L15–L27

~~~~~~~~text
Status: **Accepted (schema frozen; every class-specific value is registration-measured).**
Activates nothing. This ADR pins the *step function* of ADR-0027's one-step refutation: which
operator, which tile shape, which reduction order `shape_profile_id` binds. It is the
prerequisite ADR-0027's Consequences named for Stage 2, and the design basis for the
execution-commitment **v2** (step leg), the canonical transcendentals (ADR-0031), and
`ExecutionStepRefutationV1`.
Date: 2026-08-16
Relates to: ADR-0027 §1/§2 (the refutation this makes adjudicable; the reference arithmetic),
ADR-0026 §2 (activation/GEMM legs, "committed" → "adjudicable"), ADR-0028 §6 (Stage 2
prerequisite), ADR-0029 §3/§6 (carriage mass budgets; chunked evidence),
`consensus/core/src/palw_reference.rs` (ruleset v1 this extends),
`consensus/core/src/palw_legs.rs` (the v1 composite this versions past),
`misaka/palw/worker/src/shim.c` (the execution pins the profile must restate).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L164: Decision / L321: Mission alignment amendment — 2026-10-07

<a id="source-0031-palw-canonical-transcendentals"></a>

## 0031-palw-canonical-transcendentals.md

[原文](../adr/0031-palw-canonical-transcendentals.md) / [新しい主担当](0013-tir-arithmetic-and-determinism.md)

原文 L12–L19

~~~~~~~~text
Status: **Accepted (programs land consensus-inert; every binding is registration-measured).**
IEEE-754 does not require correctly-rounded libm, so `expf` is not one function — it is
whichever algorithm the class actually runs. This ADR pins how transcendental sites are
identified, which programs exist today, and what can never be pinned.
Date: 2026-08-16
Relates to: ADR-0030 (Fact 15, the site taxonomy and `transcendental_algorithm_id`),
ADR-0027 §2 (reference semantics), `consensus/core/src/palw_reference.rs` (ruleset v2 — the
arithmetic these programs are written in), `consensus/core/src/palw_transcendental.rs`.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L54: Decision

<a id="source-0032-palw-fee-bond-escrow"></a>

## 0032-palw-fee-bond-escrow.md

[原文](../adr/0032-palw-fee-bond-escrow.md) / [新しい主担当](0037-fee-escrow-and-role-consent.md)

原文 L15–L25

~~~~~~~~text
Status: **Accepted (design; activates nothing).** This decides HOW opening-call fees and
challenger bounties work — the piece ADR-0029 §7 deferred "needs the bond-UTXO covenant
discipline". The decision is that Stage 1 needs **no new covenant machinery at all**, and
Stage 2's escrow reuses the one covenant discipline the chain already enforces: the
consensus-recognized bond-UTXO spend gate.
Date: 2026-08-16
Relates to: ADR-0028 §4/§5 (the economics being carried: `(1+q·ρ_v)·base` issuance split,
fee-bonded audit calls, no-show slash floor = fee×100, challenger economics rivalrous by
construction), ADR-0027 §4 (slash allocation, challenger bounty ≤ 49 % under the 2026-10-10 amendment, `slash_id`
idempotence), ADR-0016/0017 (stake-locked bond UTXOs — the existing spend-gate discipline),
ADR-0029 §2 (the no-outputs rule reserving the `(tx_id, 0)` reporter slot).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L41: Decision / L96: Mission alignment amendment — 2026-10-07 / L102: PALW challenger share amendment — 2026-10-10 / L142: Bond予算・総影響保存の改定 — 2026-10-10

<a id="source-0033-palw-credit-gate-wiring"></a>

## 0033-palw-credit-gate-wiring.md

[原文](../adr/0033-palw-credit-gate-wiring.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** Withdrawn — [ADR-0037](0037-palw-async-job-state-machine.md) / [ADR-0038](0038-palw-is-the-consensus-work.md) により旧 credit overlay 方針を撤回。
~~~~~~~~

<a id="source-0034-palw-execution-class-model-band-routing"></a>

## 0034-palw-execution-class-model-band-routing.md

[原文](../adr/0034-palw-execution-class-model-band-routing.md) / [新しい主担当](0015-permissionless-model-registration.md)

原文 L15–L40

~~~~~~~~text
Status: **Accepted (architecture; activates nothing). Amended 2026-08-26 by
[ADR-0053](0053-palw-one-execution-family.md) Decision 5: the `Metal` routing family joins `Cuda`
and `Rocm` as reserved.** Nothing routed to it once the Metal/GGUF verification family was
withdrawn, and a Metal binding today would be a deterministic-integer class on Apple GPUs — a thing
this tree has no kernel for and no vendor guarantees float reproducibility for. It returns the way
CUDA does, through the ADR-0027 conformance campaign. Everything else here stands.** Devnet / shadow / zero-credit envelope
unchanged. This ADR fixes how re-verification work is **routed** once PALW carries more than one
model and more than one backend: which coarse keys a receipt and a verifier advertise, how the
two sides are matched automatically without a human picking jobs off a list, and — the part that
keeps the rest honest — which key is allowed to *decide* anything. It was circulated in draft
under the number 0028 ("Execution Class / Model Band による PALW 自動再検証ルーティング設計");
that number was already taken by the challenge-sampling protocol, which this ADR builds on
rather than replaces. One element of the draft is **rejected, not adopted** — the
`FINALIZED_WITHOUT_REPLAY` crediting path — see §7, which explains why it cannot coexist with
premises this fork has already accepted and what replaces it.
Date: 2026-08-16
Relates to: ADR-0026 §3/§8 (exact-within-class, per-class signed bundles), ADR-0027 (P1–P3,
one-step adjudication, GPU admission falsifiability), ADR-0028 (credit gate, `select_replay_panel_v1`
assignment, DAA windows, `q` as funded replays, opening-call audits), ADR-0029 (carriage kinds
0x01–0x06, the version-trap rule, mass budgets), ADR-0030 (step space, `PalwShapeProfileV3`,
`PalwStepOpKindV1`), ADR-0031 (transcendental provenance; the Apple-libm admission boundary),
ADR-0032 (fee/bounty mechanics), ADR-0033 (where `credit(C)` is evaluated),
`consensus/core/src/palw_registry.rs` (`PalwClassRegistrationV1` — the object this ADR
generalizes), `consensus/core/src/palw_schedule.rs` (`select_replay_panel_v1`,
`credited_ceiling_tokens_v1`, `PALW_SCHEDULE_REPLAY_KAPPA`), `consensus/core/src/vlt.rs`
(`derive_runtime_class_id`, the class-tag literals), the v2 design §16 (capability declaration).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L75: Decision / L551: Stage mapping (the draft's §24 onto the accepted ladder) / L636: Mission alignment amendment — 2026-10-07

<a id="source-0035-palw-public-testnet-strategy"></a>

## 0035-palw-public-testnet-strategy.md

[原文](../adr/0035-palw-public-testnet-strategy.md) / [新しい主担当](0011-release-activation-and-flag-days.md)

原文 L14–L19

~~~~~~~~text
- Status: Accepted (implemented 2026-08-17; operator items listed in §6)
- Track A gate 5. Evidence base: gates 1–4 —
  `docs/palw-algo4-forgery-audit-2026-08-16.md` (gate 1),
  `docs/palw-algo4-crosshost-determinism-2026-08-16.md` (gate 2),
  `docs/palw-algo4-difficulty-economics-2026-08-16.md` (gate 3),
  `docs/history/palw-testnet11-soak-2026-08-16.md` (gate 4, running).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L63: 2. Decision 1 — the public PALW testnet is testnet-11, the *current chain* / L82: 3. Decision 2 — class admission is pinned in code, not in a runbook / L118: 4. Decision 3 — the participation model is honest about who can join / L166: Status of the gate-6 checklist, measured 2026-08-18

<a id="source-0036-palw-mainnet-activation-model"></a>

## 0036-palw-mainnet-activation-model.md

[原文](../adr/0036-palw-mainnet-activation-model.md) / [新しい主担当](0011-release-activation-and-flag-days.md)

原文 L15–L16

~~~~~~~~text
Status: **Proposed (governance decision), with Decision 4's hash floor SUPERSEDED.**
Activates nothing, changes no code, moves no fence.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L79: Decision / L180: Mission alignment amendment — 2026-10-07

<a id="source-0037-palw-async-job-state-machine"></a>

## 0037-palw-async-job-state-machine.md

[原文](../adr/0037-palw-async-job-state-machine.md) / [新しい主担当](0005-palw-consensus-architecture.md)

原文 L15–L19

~~~~~~~~text
Status: **Superseded in part by ADR-0038 (same day).** Decision 1 (hash-PoW as the primary
consensus work; PALW never block-critical) is **reversed** by ADR-0038 — it secured the chain by
making the chain's thesis optional. Decisions 2–9 (state machine, binding, panels, court seating,
classes, mint hygiene, P_check exclusion, registry/freeze) are **carried forward** into ADR-0038
Decision G and remain normative through it. Read ADR-0038 first.
~~~~~~~~

原文 L94–L101

~~~~~~~~text
    status: PalwJobStatus,
    deadlines: PalwDeadlines,
    user_escrow_amount: Sompi,
    max_inflation_credit: Sompi,
    verdict: PalwVerdict,
    reward_claimed_bitmap: u32,
}
```
~~~~~~~~

判断・改定等を含む見出し（抽出）: L1: ADR-0037: PALW asynchronous job state machine — hash-floor direction withdrawn / L70: Decision 1 — Three layers; PALW is never block-critical on a value-bearing network / L74: Decision 2 — Jobs are state, not history / L124: Decision 3 — Identity and signatures are fully bound, verified at every consumer entry / L148: Decision 4 — Panel selection: future anchor, real snapshot, dual deadline / L169: Decision 5 — Sampled verification is the fast path, never the final ruling / L189: Decision 6 — Two-tier hardware taxonomy; classes qualify by calibration, not self-declaration / L211: Decision 7 — Mint is a carve of scheduled subsidy, never an append / L241: Decision 8 — P_check and self-declared capacity are out of the safety argument / L257: Decision 9 — On-chain class registry; freeze halts credit, not the chain; no per-job override / L273: Decision 10 — Reconciliation and the 10 BPS question / L320: Implementation order and current status (2026-08-17) / L344: Mission alignment amendment — 2026-10-07

<a id="source-0038-palw-is-the-consensus-work"></a>

## 0038-palw-is-the-consensus-work.md

[原文](../adr/0038-palw-is-the-consensus-work.md) / [新しい主担当](0005-palw-consensus-architecture.md)

原文 L17–L20

~~~~~~~~text
Status: **Accepted (architecture decision), amended.** Supersedes **ADR-0037 Decision 1** (the
layer inversion below) while carrying forward ADR-0037's machinery (Decisions 2–9) re-seated
under the new layer assignment. Everything else in ADR-0036 (lineage, new-identity requirement,
land→accept→mint separation, no per-job override) stands.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L37: Context — why ADR-0037 Decision 1 was wrong / L58: Decision A — The layer inversion / L89: Implementation status — Decision A, 2026-08-18 / L126: What Decision A still needs, in dependency order / L224: Decision C's assigned duty, and the window asymmetry found while wiring it / L275: Decision C's freeze clause — implemented, and the block was not where it looked / L354: Decision B — Acceptance now, weight later: the ramp / L386: Decision C — Verification: assigned sampling is the alarm, the court is the truth / L407: Decision D — Multi-class difficulty: per-class DAA, static PWU only inside a class / L425: Decision E — What hash still does / L429: Decision F — Fork choice, IBD, and the fabrication problem / L449: Decision G — What survives from ADR-0037, re-seated / L464: What actually blocks Decisions B and C: the carriage store cannot name a chain / L533: External audit, 2026-08-19: NO-GO — and what the status table below does not mean / L564: Implementation status — Decisions A–D and H, 2026-08-19 / L630: Decision H — Block cadence is FROZEN at one block per 120 seconds (testnet and mainnet) / L692: Invariants v2 (release-blocking; supersede ADR-0037's I1/I2/I12, carry the rest) / L789: Mission alignment amendment — 2026-10-07 / L799: Bond予算・総影響保存の改定 — 2026-10-10

<a id="source-0039-palw-only-block-production"></a>

## 0039-palw-only-block-production.md

[原文](../adr/0039-palw-only-block-production.md) / [新しい主担当](0005-palw-consensus-architecture.md)

原文 L15–L20

~~~~~~~~text
Status: **Proposed, with Decision 5 and its 2026-08-17 amendment SUPERSEDED IN PART by
ADR-0045.** Activates nothing. Supersedes **ADR-0038 W4 and W6** (restated below as W4′ and W6′)
and amends its Decision B and Decision E. Everything else in ADR-0038 — the layer inversion,
sampled verification, the court, per-class DAA, bonded producers — stands. Decisions 1–4 and 6 of
this ADR stand unchanged; Decision 5's epoch cap is re-denominated by
[ADR-0045](0045-palw-class-economy-on-chain.md) Decision 2 — see the banners on that decision.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L67: Decision 1 — The floor is a CLASS, not a hash: PALW-BASE-0 / L128: Decision 2 — W6′ (supersedes W6): PALW-only liveness / L168: Decision 3 — W4′ (supersedes W4): two derived weights, one fork choice / L246: Decision 4 — The ticket is not a hash puzzle / L274: Decision 5 — Per-class share, epoch caps, and how caps are enforced / L285: Decision 5 amendment (2026-08-17) — the clause as written cannot be enforced at admission / L289: Decision 6 — Bonded is not permissioned / L385: Mission alignment amendment — 2026-10-07

<a id="source-0040-palw-base-0-integer-arithmetic"></a>

## 0040-palw-base-0-integer-arithmetic.md

[原文](../adr/0040-palw-base-0-integer-arithmetic.md) / [新しい主担当](0013-tir-arithmetic-and-determinism.md)

原文 L12–L15

~~~~~~~~text
Status: **Proposed.** Activates nothing and registers no class. Specifies the arithmetic a
`PALW-BASE-0` implementation must reproduce bit-for-bit, and the op set it is allowed to contain.
Implementation follows this ADR; the class registers only after two independent implementations
agree and its kernel catalog is complete.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L39: Decision A — Integer-only means integer-only / L49: Decision B — Representation / L63: Decision C — The three arithmetic rules, stated once and used everywhere / L178: Decision D — The op set, closed and minimal / L208: Decision E — Reduction order is free, and this is the class's central property / L226: Decision F — The two integer transcendentals, as algorithms / L283: Decision G — What this buys the catalog / L301: Decision H — Rescale, the tenth op: a scale change that is allowed to amplify

<a id="source-0041-palw-pruning-proof-verification"></a>

## 0041-palw-pruning-proof-verification.md

[原文](../adr/0041-palw-pruning-proof-verification.md) / [新しい主担当](0012-state-root-pruning-and-snapshots.md)

原文 L12–L18

~~~~~~~~text
Status: **Landed.** Activates nothing on a shipped preset. Decision 1′ (the resident verification
agent, opt-in behind `MISAKA_PALW_AGENT=1`) and Decision 2 (bounded concurrency, `MISAKA_PALW_CONCURRENCY`,
default 1) are landed and measured; Decision 3 (the header cap) and Decision 4 (cheap checks before
the inference) were already landed. Decision 1 (sampling) is withdrawn as unsound.
Governs how a node validates a pruning-point proof and a trusted set on a network whose PoW is a
PALW inference (`pow_palw_activation` / `pow_palw_ollama_activation` active — testnet-11 and devnet
today).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L81: The decision / L87: Decision 1 — WITHDRAWN: sampling the PoW checks is UNSOUND in this proof structure / L91: Decision 1′ — Amortise the per-header cost (this is the real lever) / L232: Decision 2 — Parallelise verification during IBD — landed, and it buys far less than drafted / L308: Decision 3 — Hard header cap, enforced before any inference / L322: Decision 4 — Cheap checks gate the inference — already landed

<a id="source-0042-palw-mainnet-candidate-ruleset"></a>

## 0042-palw-mainnet-candidate-ruleset.md

[原文](../adr/0042-palw-mainnet-candidate-ruleset.md) / [新しい主担当](0005-palw-consensus-architecture.md)

原文 L15–L16

~~~~~~~~text
Status: **Proposed.** Activates nothing, moves no fence, changes no shipped preset. It is the
engineering spec the `palw-v2` branch implements before any public PALW-RC or mainnet activation.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L92: Decision 1 — One atomic activation bundle, not five fences / L144: Decision 2 — The block state machine, and why a fresh tip is weighable (closes P0-3) / L180: Decision 3 — PalwAttemptEnvelopeV2, a new algo id, and identity by attempt_id (closes P0-1) / L222: 3c. Block identity is attempt_id, not the raw signature — deferred with cause (Amendments §A2) / L268: Decision 4 — The full node runs no model (closes W1 / the runtime half of P0-8) / L290: Decision 5 — Candidate-scoped PALW state, and an authenticated commitment for pruning (closes P0-4, half of P0-5) / L342: Decision 6 — Admission split, and per-bond exposure (closes P0-2, P0-10) / L384: Decision 7 — Panel, data availability, and no-show (closes the wiring half of P0-7, DA half of P0-8) / L410: Decision 8 — The BASE-0 court is complete and proof-carrying (closes P0-8, P0-9) / L450: Decision 9 — One fork-choice authority (closes the other half of P0-5) / L491: Decision 10 — Reward is not spendable before Final (closes reward-before-void) / L508: Decision 11 — The ruleset fingerprint, committed to genesis (RC == mainnet, by hash) / L578: Amendments (2026-08-20, implementation-time) / L583: A1 — The court races window_court, not the challenge window (Decision 1, Decision 8) / L598: A2 — Decision 3c must not land without a mutated-witness path / L616: A3 — Decision 7's Sybil guarantee, restated as the bound it is / L670: Mission alignment amendment — 2026-10-07

<a id="source-0043-palw-v2-state-root-ordering"></a>

## 0043-palw-v2-state-root-ordering.md

[原文](../adr/0043-palw-v2-state-root-ordering.md) / [新しい主担当](0012-state-root-pruning-and-snapshots.md)

原文 L12–L19

~~~~~~~~text
- **Status:** Accepted (2026-08-20)
- **Relates to:** ADR-0042 Decision 5 (which required this record before any header commits a
  state root: "the ADR that lands it must spell out a hash ordering with no
  challenge↔commitment cycle — don't implement by vibes"), Decision 3 (the attempt transcript),
  Decision 9 (the comparator), Decision 11 (the ruleset fingerprint).
- **Lands with:** PR-03 (`consensus/core/src/palw_state_v2.rs`). The header FIELD that carries
  the root arrives with the V2 header format in PR-10; this ADR fixes what the bytes will mean
  so nothing about them is decided by the PR that wires them.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L174: Amendment 2026-09-02 — version 17 (ADR-0078): the derivation table

<a id="source-0044-palw-free-prompt-receipts"></a>

## 0044-palw-free-prompt-receipts.md

[原文](../adr/0044-palw-free-prompt-receipts.md) / [新しい主担当](0027-claim-lifecycle-and-liability.md)

原文 L17–L19

~~~~~~~~text
Status: **Proposed.** Activates nothing, moves no shipped preset, changes no live network. It is the
engineering spec the `palw-freeprompt-v3` branch implements on top of the `palw-v2` RC substrate
(ADR-0042, PR-00…PR-10).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L133: Decision 1 — Two work sources, one atomic bundle / L156: Decision 2 — The free-prompt job: user tokens in, nothing appended, total binding / L191: Decision 3 — The execution commitment, and certification through the existing lattice / L233: Decision 4 — The beacon rule: only attempt blocks carry randomness / L258: Decision 5 — Quantized one-shot tickets / L294: Decision 6 — The receipt block (algo 7): admission a full node runs with no model / L349: Decision 7 — Pricing: CU from the executed shape, conservative by construction / L354: Decision 8 — Data availability and privacy, v1 / L370: Decision 9 — Bundle extension, invariants, and what moves / L418: Decision 10 — The user pipeline: one inference, an answer and a commitment / L544: Mission alignment amendment — 2026-10-07

<a id="source-0045-palw-class-economy-on-chain"></a>

## 0045-palw-class-economy-on-chain.md

[原文](../adr/0045-palw-class-economy-on-chain.md) / [新しい主担当](0020-class-share-and-allocation-evolution.md)

原文 L12–L21

~~~~~~~~text
Status: **Accepted (implemented on the V2 lineage; consensus-inert — no shipped preset carries
`ConsensusV2`).**
Date: 2026-08-20
Relates to: ADR-0038 Decision D (per-class difficulty domains), ADR-0039 Decision 5 and its
2026-08-17 amendment (the four defects and the blocked enforcement point), ADR-0042 Decisions
1/5/6 (atomic bundle, candidate-scoped state, admission list), ADR-0043 (state-root ordering),
`consensus/core/src/palw_pwu.rs` (the derivation identity and `check_pwu_claim_v1`),
`consensus/core/src/palw_class_daa.rs` (the V1 budget derivation and its `StarvedClass`
refusal), `consensus/core/src/palw_state_v2.rs`, `consensus/core/src/palw_admission_v2.rs`,
`consensus/core/src/palw_mode_v2.rs`, threat model rows H3 and "Per-class DAA / lifecycle".
~~~~~~~~

判断・改定等を含む見出し（抽出）: L55: Decision 1 — pwu has exactly one legal value (the ADR-0039 derivation item, V2 face) / L88: Decision 2 — the epoch budget's currency is the block / L146: Decision 3 — the share table is chain state, granted at registration, conserved by arithmetic

<a id="source-0046-palw-v2-consensus-object-carriage"></a>

## 0046-palw-v2-consensus-object-carriage.md

[原文](../adr/0046-palw-v2-consensus-object-carriage.md) / [新しい主担当](0004-mass-carriage-and-transaction-relay.md)

原文 L12–L19

~~~~~~~~text
- **Status:** Accepted (2026-08-20)
- **Relates to:** ADR-0042 (the ruleset this feeds; its Decision 5 state machine consumes
  `PalwConsensusObjectV2`), ADR-0043 (the state root those objects move), ADR-0029 (the V1
  carriage whose Stage-1 shape this reuses), ADR-0009/0010 (the stake-bond rail this mirrors
  for bonds).
- **Lands with:** the acceptance seam (`consensus/src`) and the RC genesis loader. The wire
  types and stateless validation land now in `consensus/core/src/palw_carriage_v2.rs`,
  dormant — no shipped preset admits the band.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L31: Decision 1 — one subnetwork id per kind, band 0x50, Borsh body, no magic / L58: Decision 2 — two validation layers with two different failure meanings / L81: Decision 3 — the bond IS its collateral output / L122: Decision 4 — panels are derived, receipts are counted, courts carry their proof / L157: Decision 5 — order is acceptance order

<a id="source-0047-palw-a16-activation-tier"></a>

## 0047-palw-a16-activation-tier.md

[原文](../adr/0047-palw-a16-activation-tier.md) / [新しい主担当](0013-tir-arithmetic-and-determinism.md)

原文 L22–L24

~~~~~~~~text
**Status:** Accepted (2026-08-21). Amends ADR-0040's catalog by addition; BASE-0 itself is
untouched — its profile references none of the new ops, its class id and behavior are unchanged,
and it remains the liveness floor exactly as ADR-0039 ordered.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L53: Decision

<a id="source-0049-palw-adjudication-contract"></a>

## 0049-palw-adjudication-contract.md

[原文](../adr/0049-palw-adjudication-contract.md) / [新しい主担当](0038-g14-public-prosecution-and-completeness.md)

原文 L15–L17

~~~~~~~~text
Status: **Proposed.** Activates nothing, registers no class, and changes no proof-of-work. It
specifies what a refutation may ask for, what the court may open to answer it, and the admission
gate that makes both bounded by a class's own geometry rather than by its model's size.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L114: Decision A — operands are addressed in bytes, and the canonical encoding is pinned / L136: Decision B — the terminal adjudication is tile-local / L152: Decision C — admission bounds the court from the class's own geometry / L173: Amendment, 2026-08-26 — the ceilings are numbers now, and the metric they are on changed / L227: Decision D — coverage is over reachable coordinates, not over kernel ids / L238: Decision E — decode is adjudicable, by challenging the argmax rather than proving it / L285: Decision F — the engine, the profile, the adjudicator and the inventory are projections of one description / L350: Decision G — the canonical artifact inventory, and one meaning for "class id" / L388: Decision H — post-genesis class registration is allowed, at the minimum share, gated by Decision C / L432: Mission alignment amendment — 2026-10-07

<a id="source-0050-palw-base0-residual-site"></a>

## 0050-palw-base0-residual-site.md

[原文](../adr/0050-palw-base0-residual-site.md) / [新しい主担当](0013-tir-arithmetic-and-determinism.md)

原文 L12–L14

~~~~~~~~text
Status: **Proposed.** Changes no op, amends no catalog, and activates nothing. It fixes a declared
graph that cannot be adjudicated at its residual sites, and settles whether the residual add may
amplify.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L95: Decision A — the residual site gains its narrowing node, and this is a correctness fix / L110: Decision B — the residual may amplify, and its gain is a registration artifact / L120: Decision C — the gain is per tensor, per layer, per site — not per channel / L130: Decision D — the new parameters join the artifact inventory as named tensors / L146: Decision E — this is blocked on ADR-0049 Decision A, and the block is not incidental

<a id="source-0051-palw-metal-gguf-execution-family"></a>

## 0051-palw-metal-gguf-execution-family.md

[原文](../adr/0051-palw-metal-gguf-execution-family.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** Superseded — 2026-08-26 に ADR-0053 で撤回。
~~~~~~~~

<a id="source-0052-palw-qwen36-hybrid-class"></a>

## 0052-palw-qwen36-hybrid-class.md

[原文](../adr/0052-palw-qwen36-hybrid-class.md) / [新しい主担当](0013-tir-arithmetic-and-determinism.md)

原文 L15–L17

~~~~~~~~text
Status: **Proposed.** Registers no class and activates nothing. Specifies the ops a Qwen3.6
implementation must reproduce bit-for-bit, records what has been measured about them, and states
what is still missing before the class could carry weight.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L59: Decision A — Everything ADR-0040 says still holds / L66: Decision B — The router is a SELECTION, and its tie rule is normative / L82: Decision C — The combine has one accumulator / L87: Decision D — IntLn, the fourth transcendental / L105: Decision E — The gated delta rule, and why an integer state is stable / L137: Decision F — Partial rotation is not an optimisation / L190: Amendment (2026-08-26, same day): the two "not decided" items above are now decided / L254: Mission alignment amendment — 2026-10-07

<a id="source-0053-palw-one-execution-family"></a>

## 0053-palw-one-execution-family.md

[原文](../adr/0053-palw-one-execution-family.md) / [新しい主担当](0005-palw-consensus-architecture.md)

原文 L15–L18

~~~~~~~~text
Status: **Accepted.** Supersedes ADR-0051. Removes the Metal/GGUF execution family, the family
concept, the per-class panel, the runtime pins and the `misaka-palw-metal` crate. Moves
`PALW_STATE_V2_VERSION` (7 → 8) and the ruleset id, so testnet-11's consensus fingerprint moves and
every network re-mints.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L1: ADR-0053: One execution family — Family M is withdrawn, and the court is not optional / L165: Decision 1 — There is one execution family, and it is not a value / L177: Decision 2 — PalwClassTermsV2 is deleted, and the class record shrinks / L190: Decision 1a — What "one gate" does and does not claim, at genesis / L227: Decision 3 — One panel: the network's / L233: Decision 4 — --palw-register-class survives, pointed at a class the court can judge / L253: Decision 5 — ADR-0034's Metal routing family becomes reserved / L266: Decision 6 — What is deleted, in full / L317: Mission alignment amendment — 2026-10-07

<a id="source-0054-palw-share-follows-production"></a>

## 0054-palw-share-follows-production.md

[原文](../adr/0054-palw-share-follows-production.md) / [新しい主担当](0020-class-share-and-allocation-evolution.md)

原文 L18–L20

~~~~~~~~text
Status: **Accepted.** Adds the share-raise path ADR-0045 deferred ("automatic share re-allocation
from class health") and closes the half of ADR-0045 Decision 3 that had no writer. Moves
`palw_ruleset_id_v2` (two new `PalwStateParamsV2` fields), so every network re-mints.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L65: Decision 1 — production earns cadence, silence returns it / L89: Decision 2 — the floor keeps a reserve, and it is a number / L101: Decision 3 — where it runs, and what it reads

<a id="source-0055-palw-position-is-earned-not-declared"></a>

## 0055-palw-position-is-earned-not-declared.md

[原文](../adr/0055-palw-position-is-earned-not-declared.md) / [新しい主担当](0006-fork-choice-participation-and-settlement.md)

原文 L15–L19

~~~~~~~~text
- Status: Accepted
- Date: 2026-08-27
- Supersedes: nothing. Extends ADR-0044 Decision 6; reverses one relaxation recorded in
  `palw_state_v2.rs` (the court's opening rung).
- Declared by: `PALW_ATTEMPT_V2_VERSION` 4 → 5.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L53: Decision / L91: Mission alignment amendment — 2026-10-07

<a id="source-0056-palw-permissionless-class-admission-and-share-economy"></a>

## 0056-palw-permissionless-class-admission-and-share-economy.md

[原文](../adr/0056-palw-permissionless-class-admission-and-share-economy.md) / [新しい主担当](0015-permissionless-model-registration.md)

原文 L15–L19

~~~~~~~~text
**Status:** Accepted and **implemented** (2026-08-27). Decisions 1, 2 and 7 were already the
shipped design; Decisions 3 and 5 landed with the state version 8 → 9 this ADR predicted, and
Decision 6 is a policy the gate keeps by not deduplicating. **Decision 4 is withdrawn** in favour
of ADR-0054 Decision 1, which answers the same question with a proportional step instead of a
streak — see that decision for the measurement that settled it.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L113: Decision 1 — The constitution: admission is arithmetic, and only arithmetic / L130: Decision 2 — The kernel boundary: what needs a binary, and what does not / L146: Decision 3 — Registration exposure: entry is priced in bonded collateral / L171: Decision 4 — Withdrawn: the share walk is ADR-0054's, not this one's / L175: Decision 5 — Reclamation: dead classes give the network back / L202: Decision 6 — Duplicates are priced, not policed / L216: Decision 7 — What the chain does not judge / L272: Mission alignment amendment — 2026-10-07 / L280: Bond予算・総影響保存の改定 — 2026-10-10

<a id="source-0057-palw-base0-runtime-acceleration"></a>

## 0057-palw-base0-runtime-acceleration.md

[原文](../adr/0057-palw-base0-runtime-acceleration.md) / [新しい主担当](0014-versioned-kernels-and-backends.md)

原文 L12–L14

~~~~~~~~text
Status: **Accepted.** Consensus-inert: no class id, no catalog entry, no ruleset field and no
fingerprint moves under anything this ADR permits. A change that would move one is, by that fact,
outside this ADR's scope.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L53: Decision 1 — the semantic boundary is the catalogued kernel / L66: Decision 2 — no kernel certificates / L81: Decision 3 — the gate is differential, per backend, and it must fire / L96: Decision 4 — the order of work / L118: Decision 5 — what the survey suggested that is refused by name / L130: Decision 6 — fusion stops at the committed row

<a id="source-0058-palw-merged-work-is-counted"></a>

## 0058-palw-merged-work-is-counted.md

[原文](../adr/0058-palw-merged-work-is-counted.md) / [新しい主担当](0006-fork-choice-participation-and-settlement.md)

原文 L15–L20

~~~~~~~~text
- Status: Accepted
- Date: 2026-08-27
- Depends on: ADR-0038 (PALW is consensus work), ADR-0039 (per-class DAA), ADR-0045/0056
  (share economy), ADR-0054 (share follows production)
- Supersedes: nothing; amends the transition's step 4 and the meaning of "produced" everywhere
  a counter reads it
~~~~~~~~

判断・改定等を含む見出し（抽出）: L48: The decision / L128: Mission alignment amendment — 2026-10-07

<a id="source-0059-the-10b-premine-cap"></a>

## 0059-the-10b-premine-cap.md

[原文](../adr/0059-the-10b-premine-cap.md) / [新しい主担当](0010-genesis-emission-and-bond-free-bootstrap.md)

原文 L14–L19

~~~~~~~~text
- Status: Accepted
- Date: 2026-08-30
- Depends on: ADR-0042 (the RC atomic bundle and its genesis registry), audit C-08 (a bond's
  collateral is money the genesis UTXO set really holds)
- Supersedes: the per-network premine split of 2026-08-26 (mainnet 13B / re-minted test nets
  10B) and the 2026-06-17 vault layout (40 × 0.1B custody UTXOs + one main wallet)
~~~~~~~~

判断・改定等を含む見出し（抽出）: L21: The operator's decision, verbatim

<a id="source-0060-the-liveness-doctrine"></a>

## 0060-the-liveness-doctrine.md

[原文](../adr/0060-the-liveness-doctrine.md) / [新しい主担当](0009-heartbeat-floor-and-liveness.md)

原文 L17–L35

~~~~~~~~text
- Status: Accepted as doctrine; **implemented 2026-08-30, SHIPPED OFF the same day, and
  RE-IMPLEMENTED 2026-08-31 per ADR-0066** — the mainnet audit
  (`docs/palw-mainnet-audit-2026-08-30.md`) found four structural defects in Decisions 1–2 and one
  in Decision 4. ADR-0066 closes F1, F3b and F4 (the lane's price left `header.bits`; the evidence
  walk is deleted) and records F3a as open and F2 as staged. **Both features still ship OFF, but
  they are now `Params::palw_heartbeat` and `Params::palw_inactivity_leak` — top-level fences an
  operator can arm on a running network, not constants that need a rebuild.** The switches this
  status line used to name are gone: `PALW_HEARTBEAT_LANE_ENABLED` no longer exists and
  `DnsParams.inactivity_leak_daa` is retired at `u64::MAX` permanently. §12 records what must
  change before either turns on; ADR-0066's "What landed" section supersedes it for D1/D2. Decisions 3 and 5 are unaffected and live. Originally: **implemented 2026-08-30** (`palw-genesis-10b-cap` branch, same re-mint
  train as ADR-0059) — see §11 for what landed and where the implementation amended this text.
  The one operator-tunable decision is the finality leak's time constant (§6).
- Date: 2026-08-30
- Depends on: ADR-0038 (PALW is consensus work), ADR-0045/0046 (class economy; carriage),
  ADR-0054 (share follows production; permissionless admission), ADR-0056 (share economy),
  ADR-0058 (merged work is counted), ADR-0059 (the 10B premine cap)
- Amends: ADR-0038's pure-PALW production stance and testnet-11's published "there is no hash
  lane to fall back to" doctrine — a *bounded, near-weightless* hash lane re-enters, as the
  chain's clock and nothing else.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L84: 3. Decision 1 — the heartbeat lane (new) / L114: What implementation added to Decision 1 / L133: 4. Decision 2 — the emergency ramp (new) / L156: 5. Decision 3 — producer-bond self-healing is now unconditional (mostly landed) / L175: 6. Decision 4 — the finality inactivity leak (new; overlay-scoped) / L213: 7. Decision 5 — refusal gates decay (partially landed) / L333: Mission alignment amendment — 2026-10-07

<a id="source-0061-zero-seat-genesis-and-right-sized-collateral"></a>

## 0061-zero-seat-genesis-and-right-sized-collateral.md

[原文](../adr/0061-zero-seat-genesis-and-right-sized-collateral.md) / [新しい主担当](0010-genesis-emission-and-bond-free-bootstrap.md)

原文 L17–L23

~~~~~~~~text
- Status: Accepted; implemented 2026-08-30 (`palw-genesis-10b-cap` branch, same re-mint train
  as ADR-0059/0060)
- Depends on: ADR-0060 (the heartbeat lane is what makes both decisions safe), the
  post-genesis bond carrier ("a stranger can register their own bond"), ADR-0059 (the 10B cap
  the collateral re-size returns money under)
- Supersedes: the born-licensable rule (`PanelCannotBeSeated`) of the RC genesis gate, and the
  0.1B-per-seat genesis collateral both ADR-0059 and ADR-0060 §8 carried forward
~~~~~~~~

判断・改定等を含む見出し（抽出）: L36: The two decisions / L122: Mission alignment amendment — 2026-10-07

<a id="source-0062-data-availability-court"></a>

## 0062-data-availability-court.md

[原文](../adr/0062-data-availability-court.md) / [新しい主担当](0039-data-availability-court-and-panel-da.md)

原文 L25–L25

~~~~~~~~text
Status: **Implemented behind `Params::palw_da_court`, dormant** (the amended form landed 2026-09-02; SA-7 widened the same fence 2026-09-03 — see the implementation sections below). The "Proposed, not landed" reading that stood in this line and in the README until 2026-09-05 was stale. Supersedes nothing; completes ADR-0042 Decision 7.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L87: Decision / L219: Security amendment (2026-09-02) — hardening before implementation / L275: Second security amendment (2026-09-03) — SA-7: what an accusation costs, what it buys, and what it may not suspend / L416: Implementation, 2026-09-02 — the amended form, behind palw_da_court / L618: Mission alignment amendment — 2026-10-07 / L628: MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

<a id="source-0063-operator-tooling-the-missing-half"></a>

## 0063-operator-tooling-the-missing-half.md

[原文](../adr/0063-operator-tooling-the-missing-half.md) / [新しい主担当](0029-remote-client-mining-and-host-sandbox.md)

原文 L17–L17

~~~~~~~~text
Status: **Proposed** (2026-08-30).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L90: Decision / L140: D3. misaka bond status — the outpoint, from the chain / L206: Security amendment (2026-09-02) — hardening before the open decisions are built

<a id="source-0064-trustless-recovery-from-a-total-stop"></a>

## 0064-trustless-recovery-from-a-total-stop.md

[原文](../adr/0064-trustless-recovery-from-a-total-stop.md) / [新しい主担当](0009-heartbeat-floor-and-liveness.md)

原文 L14–L19

~~~~~~~~text
Status: **Partially superseded by its own correction** (2026-08-30). It was written to answer the
one item ADR-0060 §12 left open after the audit withdrew the heartbeat lane. **It does not answer
it** — see the correction under §Decision, verified against the code and the reason the title is now
wrong. What it does deliver is landed and dormant: a bond becomes usable one chain block earlier,
and the pipeline stops disagreeing with the state machine about which state answers *is this bond
registered*. Facts A and B stand on their own.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L67: Decision — no new lane. Move one lookup. / L146: Why it does not reproduce the four withdrawn findings

<a id="source-0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else"></a>

## 0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md

[原文](../adr/0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) / [新しい主担当](0031-panel-independence-and-unavailability.md)

原文 L21–L28

~~~~~~~~text
Status: **D1, D2 and D4 LANDED (all dormant); D3 and D5 decided** (2026-08-30). **D1 made
ARMABLE 2026-08-31** — see the correction below. Closes the CRITICAL recorded in the two addenda to
`docs/palw-mainnet-audit-2026-08-30.md`. **Mainnet blocker.** Consensus-affecting, but **no
re-mint for the RULES**: both landed rules sit behind top-level `ForkActivation` fences, `None` on
every preset, so no shipped fingerprint moves and either can be armed by rolling deploy. The earlier
"fingerprint move and a re-mint on every network" in §Consequences was written before the placement
was worked out and is corrected there. **Growing the registry so D1 can be armed at all is a
separate change and it IS a re-mint** — testnet-11 Relaunch 4, below.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L89: Decisions / L440: Mission alignment amendment — 2026-10-07

<a id="source-0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table"></a>

## 0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md

[原文](../adr/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) / [新しい主担当](0009-heartbeat-floor-and-liveness.md)

原文 L12–L15

~~~~~~~~text
Status: **Decisions 1, 2 and 4's fence LANDED (2026-08-31), both dormant. Decision 3 LANDED through ADR-0068 Phase 1 (`Params::palw_attempt_work_fence`: an attempt block's blue work is the constant `1 << PALW_ATTEMPT_BLUE_WORK_LOG2`, a heartbeat's is ε — ghostdag/protocol.rs names this ADR's Decision 3 as closed). Decision 4's committed per-validator table is NOT pursued: it belongs to the validator lineage, which is retired (recorded 2026-09-05).** Supersedes the *implementation* of ADR-0060 Decisions 1–2
and Decision 4; the doctrine those decisions state is unaffected. Both features still ship OFF —
but they ship off behind **fences that can be armed**, which is the difference this ADR was written
to make. See "What landed" at the foot of this document for exactly what is and is not in the tree.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L59: Decision 1 — the lane gets its own algorithm id, and its price never touches bits / L84: Decision 2 — the slot rule is one block deep, and the evidence walk is deleted / L99: Decision 3 — ε stops competing with a V2 block's work / L110: Decision 4 — the inactivity leak needs committed per-validator state / L226: Security amendment (2026-09-02) — Decision 4's committed table, before it is built

<a id="source-0067-classes-are-chain-data-kernels-are-the-build"></a>

## 0067-classes-are-chain-data-kernels-are-the-build.md

[原文](../adr/0067-classes-are-chain-data-kernels-are-the-build.md) / [新しい主担当](0014-versioned-kernels-and-backends.md)

原文 L17–L34

~~~~~~~~text
Status: **Decisions 1–3 and 5 LANDED for the dense (A16) container (2026-08-31), fenced and
dormant; Decision 4 needed no code; Decision 6 including its cache bound LANDED (2026-09-01), as did
the pruning-point sidecar and the cross-architecture clause. The mmap (Qwen3.6) interpreter's blocker —
a measured defect in the class's registered graph — is CLOSED by the corrected `graph-v2` row, and the
interpreter itself is BUILT and differential-tested (2026-09-01), a pass that convicted a FOURTH
graph defect on the way; the mmap container's chain-resolve arm is BUILT behind the same fence
(same day — the registered root decides the container, and the mmap engine serves the declaration
bit-identically to its compiled path); the epsilon disposition is TAKEN (same day — the corrected
rows are `graph-v3`, declaring the artifact's measured epsilon, after the graph author's review
convicted a SIXTH defect that folded into the same id move); the mmap profile fuzz gate is BUILT
and run to saturation with zero findings (same day — and its design pass convicted a reachable
walk panic before the first iteration ran); only the operational arming remains PROPOSED.** See
"What landed" at
the foot of this document. Builds on ADR-0049 (the adjudication contract — whose admission
carriage is the load-bearing half of this design and is ALREADY LIVE), ADR-0054 (share follows
production — the economics that make permissionless classes survivable), and ADR-0034 (capability
declarations — the opt-in that makes them safe to serve). Consistent with the standing doctrine
that consensus changes ship by activation, never by re-genesis.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L98: Decision 1 — the chain state is the class catalog; the compiled table is genesis bootstrap plus cache / L111: Decision 2 — execution from the registered profile / L143: Decision 3 — the kernel set is the consensus surface, and it is irreducible / L166: Decision 4 — distribution and service stay off-chain, and the chain keeps pinning only identity / L179: Decision 5 — the interpreter ships behind a fence, and the fence's arming condition is a fuzz gate / L202: Decision 6 — four storage tiers, and node storage is never consensus state / L634: Security amendment (2026-09-02) — before the interpreter is armed / L726: Mission alignment amendment — 2026-10-07 / L735: Model acquisition and capital-allocation amendment — 2026-10-10後続改定 / L757: MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

<a id="source-0068-the-llm-primary-economy-and-the-floors-minimum"></a>

## 0068-the-llm-primary-economy-and-the-floors-minimum.md

[原文](../adr/0068-the-llm-primary-economy-and-the-floors-minimum.md) / [新しい主担当](0020-class-share-and-allocation-evolution.md)

原文 L15–L64

~~~~~~~~text
- Status: Accepted; **Phase 1 implemented AND drilled 2026-09-01**. F3a and F2 are closed in
  code (fences shipped OFF everywhere but devnet, which arms them as the standing drill
  network); the deterministic drill (`the_heartbeat_clock_sweeps_a_stopped_chain_back_to_life`)
  releases the block-600 wedge in 4,300 clock ticks; the live two-node drill
  (`scratchpad hb-drill/REPORT.md` of the implementing session) PROVED the lane on real
  processes — a zero-bond devnet born and bootstrapped over heartbeats, the nominal hour held
  to the second twice, 120 s recovery cadence, ε = 1 vs 2²⁰ priced exactly as declared, and
  unattended producer re-entry twice. Drill findings: **F1** (producer/heartbeat-miner SIGTERM
  hang — fixed on this branch, the panel's `tick()` copied to both), **F2** (no cadence pacing
  at genesis-permissive targets — first-epoch mint runaway on fresh nets, Phase 2 item),
  **F3** (a dying producer accepts its own block after its P2P server stops — narrowed by the
  F1 fix, residual noted), **F4** (the V2 assembly violates upstream pruning depth-consistency,
  and LIVE testnet-11 does at 6,600 = 11 × 600 — devnet fixed, t11's correction rides the
  Phase 2 flag day), **F5** (a heartbeat CHAIN deeper than `max_per_mergeset` strands
  permanently once a heavier bonded fork puts it in the anticone — **closed the same day by
  amending the width rule**: at most four heartbeats, or any number provided they form ONE
  chain (sorted by blue score, every adjacent pair ancestor-related). A chain is the lane
  doing its job through a long outage, already rate-priced by the slot ladder; width —
  siblings, and the tree shape that would fool a chain-HEAD count — is what F3a is about, and
  the pairwise total order refuses both. The rule's semantic version rides the fingerprint
  label (`palw_heartbeat/width-chain-exempt-v2`), so only armed presets moved: t11's
  scheduled fingerprint 05df4e5e… → 0533c8ee…, devnet's 47ba789e… → 280db9e1…. The drill's
  exact strand shape is the regression test).
  **F6 (the launch audit's finding, fixed before the train left):** the heartbeat MINER
  consulted `should_mine`, which folds in `is_nearly_synced` — and the sync-rate escape that
  clause leans on itself expires once the finality point is over three finality durations old.
  So the longer a chain had been stopped (or the older a fresh re-genesis's timestamp), the
  more firmly the clock refused to start: the ADR-0060 §1 self-referential hostage, wearing a
  mining-heuristic costume, masked in the drill by `--enable-unsynced-mining` on the H node.
  The miner now holds only on what a stalled chain can still supply — peer connectivity, the
  chain-participation gate, and the transitional-IBD check — and its first tick is what makes
  every unmodified producer's `should_mine` pass flag-free again.
  Phase 0 is operations and began the same day. **Phase 2 is implemented on this branch
  (`palw-adr0068-phase2`, the Relaunch 5 identity `d7510c7a…`)**: the floor reserve
  500‰ → 20‰ (one field, every V2 network), genesis shares 489/489 so the floor lands
  exactly on its reserve, both fences armed from genesis (mainnet's assembly arms the same
  way the day its card is pinned), and F4's depth-consistency nudge moved into the shared
  assembly. Note the measured correction to Phase 0's own text: the LIVE reserve was 500‰
  (`BASE_CLASS_RESERVE_PERMILLE`, which the bundle builder writes over the struct default of
  100), so the walk could never pass the half-table on the running chain — the 90% figure was
  only ever reachable through this train. Execution = `docs/history/testnet11-relaunch5-runbook.md`,
  gated on the Phase 1 fleet deploy and the operator's re-genesis go (every host wipes;
  seat 5 re-keys off the lost 160.16 host, returning unmanned seats to ≤ 2).
- Date: 2026-09-01
- Depends on: ADR-0045 (class economy: block-denominated budgets, share table),
  ADR-0054 (share follows production), ADR-0058 (merged work is counted),
  ADR-0060 (the liveness doctrine), ADR-0064 (trustless recovery), ADR-0066 (heartbeat out of
  bits — findings F2 and F3a, which this ADR closes)
- Amends: testnet-11's de-facto floor-majority cadence (share table 550/200/250) and
  `min_base_class_share_permille = 100`.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L203: Mission alignment amendment — 2026-10-07

<a id="source-0069-e2e-adjudicability-is-the-price-of-weight"></a>

## 0069-e2e-adjudicability-is-the-price-of-weight.md

[原文](../adr/0069-e2e-adjudicability-is-the-price-of-weight.md) / [新しい主担当](0038-g14-public-prosecution-and-completeness.md)

原文 L15–L28

~~~~~~~~text
Status: **ACCEPTED — IMPLEMENTED (2026-09-01; reviewed and amended 2026-09-02).** Landed on
`palw-adr0068-phase2` in `cfe1feeb` (the certificate, the root, the gate, the zero-share state),
`1ac8f3f3` (drill and grade split into two verbs), `d2e63039`/`62da1a61` (the model tiers certify
and take their cadence back), `f23f1348` (the genesis invariant test can see genesis classes),
`758c1568` (certification buys weight, not existence), and the two review fixes recorded in
"What landed". Written against the ADR-0068 launch audit, whose central
finding was that Relaunch 5's genesis share table hands **97.8% of cadence** (QWEN36 489‰ +
QWEN25-A16 489‰) to families that carry no court responder — `supports_court()` is `false` for
both, `bisect_prefix_state`/`refutation_for_index` take the trait's defaults, and a producer that
never runs the model cannot be convicted (audit F2/F6/F8). Builds on ADR-0039 (a class is
weightless until its kernel catalog closes), ADR-0049 (the adjudication contract), ADR-0067
(classes are chain data; only kernels are the build), and ADR-0054/0056 (share follows production;
permissionless admission). Consistent with the standing doctrine that consensus changes ship by
activation, never by re-genesis.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L91: 3. Decision / L93: Decision 1 — Two adjudicability properties, named apart / L104: Decision 2 — E2E certification is a build fact, committed like the catalog / L116: Decision 3 — The certification drill, defined / L143: Decision 4 — The graph must not lie about the engine (ADR-0049 Decision F, enforced) / L153: Decision 5 — The admission gate grants weight only to certified families / L167: Decision 6 — Permissionlessness and the doctrine are preserved / L332: Security amendment (2026-09-02) — the open item is a fork-choice hole, closed as Decision 7 / L490: Mission alignment amendment — 2026-10-07

<a id="source-0070-the-model-tiers-step-spaces-are-adjudicable"></a>

## 0070-the-model-tiers-step-spaces-are-adjudicable.md

[原文](../adr/0070-the-model-tiers-step-spaces-are-adjudicable.md) / [新しい主担当](0018-long-context-measurement-and-admission.md)

原文 L15–L21

~~~~~~~~text
Status: ACCEPTED (implementation on `palw-step-space-e2e`; arming is a deployment decision, §7)
Date: 2026-09-01 (numbered 0070 on merge: ADR-0069 was taken by the
launch audit's `e2e-adjudicability-is-the-price-of-weight`, which the consensus
implementation cites throughout — this one moved because only one citation had to.)
Relates: ADR-0030 (the step space), ADR-0038 A4 (coverage), ADR-0049 (canonical IR, Decisions B/C/F/G),
ADR-0052 (the hybrid's integer arithmetic), ADR-0053 (one execution family), ADR-0067 (classes are
chain data), ADR-0068 (the LLM-primary economy)
~~~~~~~~

判断・改定等を含む見出し（抽出）: L57: 2. Decision — the acceptance test IS the property / L199: Mission alignment amendment — 2026-10-07

<a id="source-0071-the-attempt-lanes-price-and-the-tickets-bound"></a>

## 0071-the-attempt-lanes-price-and-the-tickets-bound.md

[原文](../adr/0071-the-attempt-lanes-price-and-the-tickets-bound.md) / [新しい主担当](0028-attempt-ticket-and-seed-anchor.md)

原文 L15–L21

~~~~~~~~text
Status: **IMPLEMENTED WITH DECISION 1 WITHDRAWN (2026-09-02; proposed 2026-09-01).**
Decision 1's target freeze shipped to the public testnet, was measured to remove the only control on
block interval, and is reverted — see §3. Decisions 1a, 2 and 3 stand (Decision 2's pwu divisor is
superseded by ADR-0072, which takes the nonce out of the ticket altogether; the bucket itself
stands as the anchor's position field). Rejected variants retain only the reason and successor here;
the old specification is in Git history. Decision 1a's relative expectation / idle ceiling and
Decision 3's capability filtering remain implemented.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L78: 3. Decision 1 — The attempt lane's price comes off header.bits / L97: Decision 1 — WITHDRAWN after Relaunch 5 measured it (2026-09-02) / L101: Decision 1a — AMENDED at implementation: the expectation stays relative, and the repair is a ceiling / L131: 4. Decision 2 — The ticket is bound to executions, not to tries / L135: 5. Decision 3 — A panel seat must be able to run the class it judges / L247: Security amendment (2026-09-02) — Decision 3 gets a bound and a price, and §5's open item a chain-checkable form / L273: Mission alignment amendment — 2026-10-07

<a id="source-0072-the-ticket-is-the-execution"></a>

## 0072-the-ticket-is-the-execution.md

[原文](../adr/0072-the-ticket-is-the-execution.md) / [新しい主担当](0028-attempt-ticket-and-seed-anchor.md)

原文 L15–L23

~~~~~~~~text
Status: **IMPLEMENTED (2026-09-02); Decision 7's rollout DECIDED the same day — see §3.** Reviewed the same day
by a second session, whose finding became Decision 8 and the invariant in §4. Builds on ADR-0042 (Decision 3a: the algo-6 tag is an
expansion, never an inference), ADR-0044 (Decision 4: the receipt lane's quantum ticket is a
function of the certified execution), ADR-0045 (`DerivedV1`: pwu is derived from the class target),
ADR-0058 (merged work is counted) and ADR-0071 (Decision 1 withdrawn — `bits` stays the absolute
rate control; Decision 2 — the nonce bucket). Supersedes ADR-0071 Decision 2's pwu divisor; keeps
its bucket. Consistent with the standing doctrine that consensus changes ship by activation or
version, never by re-genesis: this is a version bump (`PALW_ATTEMPT_V2_VERSION` 5 → 6) and a
coordinated upgrade, not a re-mint.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L51: 2. Decisions / L220: Security amendment (2026-09-02) — the mainnet activation path, before it is built / L242: Security amendment, second pass (2026-09-03) — what the implementation measured / L313: Security amendment, third pass (2026-09-03) — the gate the sweep could not see / L361: Mission alignment amendment — 2026-10-07

<a id="source-0073-real-demand-work-bears-the-weight"></a>

## 0073-real-demand-work-bears-the-weight.md

[原文](../adr/0073-real-demand-work-bears-the-weight.md) / [新しい主担当](0026-canonical-work-and-answer-accounting.md)

原文 L21–L26

~~~~~~~~text
**Status:** PROPOSED (2026-09-02). Decision 2 is DECIDED; its activation is gated by Decisions 1 and 3.
**Builds on:** ADR-0044 (the free-prompt lane), ADR-0066 (heartbeat out of `bits`), ADR-0068 (the LLM-primary
economy), ADR-0069 (adjudicability is the price of weight), ADR-0070 (the model tiers' step spaces),
ADR-0071/0072 (the attempt lane's price and ticket).
**Amends:** ADR-0044 Decision 4's "the receipt lane is weightless" (after Decisions 1 and 3 it is not);
ADR-0069's certification scope (a family is certified for the lane it was drilled on).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L86: 2. Decisions / L257: Security amendment (2026-09-02) — preconditions on Phase ④ (Decision 4) and on 4b's supply metric / L325: Mission alignment amendment — 2026-10-07

<a id="source-0074-the-attempt-is-a-claim-drawn-by-the-chain"></a>

## 0074-the-attempt-is-a-claim-drawn-by-the-chain.md

[原文](../adr/0074-the-attempt-is-a-claim-drawn-by-the-chain.md) / [新しい主担当](0028-attempt-ticket-and-seed-anchor.md)

原文 L23–L35

~~~~~~~~text
**Status:** PROPOSED (2026-09-02), decided by the user on two points that this ADR only spells
out: (1) the attempt lane adopts the beacon draw (ADR-0073 Decision 3c's open option), and
(2) **the beacon is a fact derived from the chain a node already holds — never an attestation, a
quorum, a validator set, or a finality overlay.** The last beacon this project built was wired to
validators and the design became BFT-dependent; this one is forbidden from reading anything a
validator says. Ships in the Relaunch 5 re-genesis (5d) together with ADR-0072 + D8 and
ADR-0073 Phase ①, deployed as a rolling host swap (the fingerprint moves, so un-wiped peers are
refused at handshake and cannot re-feed the old chain).
**Builds on:** ADR-0044 (the receipt lane: commit, beacon, quantum, spend), ADR-0072 (+ D8: the
self-drawn ticket and its pins), ADR-0073 (court → unit → weight → share).
**Amends:** ADR-0073 Decision 3 (this is the "one unit" and the "lottery discipline" made
concrete), ADR-0044 Decision 7 (CU weights withdrawn), ADR-0071/0072's premise that the attempt
lane is the primary lane.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L65: 2. Decisions / L208: Mission alignment amendment — 2026-10-07

<a id="source-0075-certification-is-a-consensus-object"></a>

## 0075-certification-is-a-consensus-object.md

[原文](../adr/0075-certification-is-a-consensus-object.md) / [新しい主担当](0032-sampling-beacon-and-receipts.md)

原文 L17–L26

~~~~~~~~text
**Status:** PROPOSED (2026-09-02), implemented on branch `palw-adr0073-fp-weight` for the
Relaunch 5e re-genesis (the state version and the genesis free-prompt set move, so the fingerprint
moves). Written against the goal "the public testnet accepts free-prompt claims on the real
models, and a model's certification no longer needs a code change".
**Builds on:** ADR-0069 (end-to-end adjudicability: weight is the price of a certified family),
ADR-0074 Decision 6 (the free-prompt lane bears weight only on a certified class), ADR-0054
(permissionless admission), ADR-0042 Decisions 7/8 (lifecycle objects ride transactions).
**Amends:** ADR-0069 Decisions 2 and 5 (the certified set is no longer only the build's), ADR-0069
Decision 6 (a weightless entrant is seated by an object, not by a re-genesis), ADR-0074 Decision 6
(the free-prompt-certified set has a chain half).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L57: 2. Decisions / L151: 6. The mainnet route for a model this build never pinned (Decision 8) / L182: 7. Mainnet: the rules of operation (Decisions 9–13) / L256: Security amendment (2026-09-02) — the permissionless lane's griefing budget / L368: Mission alignment amendment — 2026-10-07

<a id="source-0076-the-attempt-lanes-seed-is-the-retargets-equilibrium"></a>

## 0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md

[原文](../adr/0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) / [新しい主担当](0008-daa-difficulty-and-consensus-clock.md)

原文 L18–L28

~~~~~~~~text
**Status:** ACCEPTED (2026-09-02), implemented for the Relaunch 5e re-genesis (every preset's
fingerprint moves). Written against a measurement, not a design review: testnet-11 Relaunch 5d ran
its first hour with the liveness floor at 249 blocks and both model tiers at **zero**, against a
share table that allots them 489‰ each.
**Builds on:** ADR-0038 Decision D (per-class difficulty domains; wall-clock never enters fork
choice), ADR-0045 Decision 1 (`pwu_per_inference` is the canonical job's counted step leaves),
ADR-0071 Decision 1 as amended (block interval is `calculate_difficulty_bits`' job; the per-class
retarget only redistributes share), ADR-0072 (the ticket is the execution — one canonical
inference is one draw).
**Makes reachable:** ADR-0075 (a stranger's model, certified on-chain, has to be able to produce
once it is seated).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L72: 3. Decision 1 — the seed is share · pwu, against one pinned scale / L97: 4. Decision 2 — the scale is spent on headroom / L124: 5. Decision 3 — the seed is written at genesis assembly, against the EFFECTIVE share table / L141: 5b. Decision 4 — a class being SEATED is a class being priced / L202: Amendment (2026-09-02) — §8 restated against the shipped rule

<a id="source-0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try"></a>

## 0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md

[原文](../adr/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) / [新しい主担当](0024-inference-input-output-contract.md)

原文 L15–L37

~~~~~~~~text
**Status:** PROPOSED (2026-09-02). Written against a measurement, not a design review: the
network's stated purpose is a person using their own local LLM on a prompt of their own, with that
one inference mining (ADR-0044 §Context, ADR-0073 §1), and on the live testnet-11 (Relaunch 5e)
the widest such prompt a registered class admits is **eight tokens, answer included**, on a lane
that holds 100‰ of the cadence, weighs nothing as a block, and has never been run on the fleet.
Nothing here is armed by this document. Its one requirement is R0 (§2): the practical local
LLM and the mining runtime are the SAME inference — not two binaries that each hold half of the
purpose. Phase A is executor-side and consensus-inert. Phase B
moves the ruleset (a re-genesis on testnet-11, as every relaunch has been; mainnet ships PALW off
and is untouched). Phase C is ADR-0073 Decision 4's activation, sequenced here and not re-decided.
Phase D is refused by admission until the ruleset move that carries its rules.
**Builds on:** ADR-0044 (the receipt lane; F1–F16), ADR-0049 Decisions C/E/F (admission bounds
the court from the class's geometry; decode is adjudicable through the tiled logits pin; engine,
profile, adjudicator and inventory are one description), ADR-0070 (the model tiers' step spaces
are adjudicable end to end), ADR-0073 (Phase ① landed; Decision 2 decided; Phase ③ landed through
ADR-0074 Decision 5; Phase ④ pending), ADR-0074 Decisions 1 and 5 (`User` and `Canonical` prompt
modes; the quantum is an eighth of the canonical job), ADR-0075 (certification is a consensus
object; a class is seated by `ClassLaneCertified`), ADR-0076 (the attempt seed is `share · pwu`;
the receipt lane seeds on its own), ADR-0026 (borrow Ambient's architecture, refuse its proof
model), ADR-0028 (sampling decides nothing; only the court convicts).
**Amends:** ADR-0044 Decision 8 (PublicDA is the only weight-bearing mode) and Decision 10 (v1 is
non-streaming); the "streaming" and "encrypted prompts" rows of ADR-0044's *not decided* list;
ADR-0073 Phase ① 1e (a seat hashes the whole capture), on the free-prompt lane.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L168: 3. Decisions / L504: Security amendment (2026-09-02) — hardening before Phases A–D are built / L568: Implementation note, 2026-09-06 — Decision 16's transport (f7363db9) / L603: Mission alignment amendment — 2026-10-07

<a id="source-0078-what-was-made-from-it-is-committed-the-thing-never-rides"></a>

## 0078-what-was-made-from-it-is-committed-the-thing-never-rides.md

[原文](../adr/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) / [新しい主担当](0025-context-and-input-commitments.md)

原文 L15–L28

~~~~~~~~text
**Status:** PROPOSED (2026-09-02). Written to draw a line ADR-0077 needed drawn: implementing
ADR-0077 makes an inference at a usable width a certified, spendable claim. It does NOT make
"generate a 3D model with Qwen3.6, verified end to end" true. The mesh is not the inference. This
ADR is the layer above the receipt — the model's output turned into a thing a person keeps — and
its rule for the chain is the title. Nothing here bears weight, moves a preset, or changes how a
claim is certified; the consensus surface is one compact object per derivation, and it is
introduced in a ruleset move of its own.
**Builds on:** ADR-0077 (R0: one inference is one answer and one claim; R1: the artifact to the
user, the receipt to the chain), ADR-0044 (the receipt lane; `output_root` is the commitment to
the output ids, never the tokens), ADR-0074 Decision 1 (the `User` prompt mode), ADR-0075
(lifecycle objects; `ObjectChunk` for anything above one carrier), ADR-0053 (one execution family:
pure Rust in the tree, integer arithmetic, because float is unadjudicable), ADR-0026 (borrow the
architecture, refuse the tolerant proof model).
**Withdrawn:** independent PALW weight for a transformer's computation (§3 Decision 7). Derivation objects remain weightless under ADR-0144 P3.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L90: 3. Decisions / L415: Security amendment (2026-09-02) — hardening before the kinds are built / L464: Mission alignment amendment — 2026-10-07

<a id="source-0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host"></a>

## 0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md

[原文](../adr/0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md) / [新しい主担当](0029-remote-client-mining-and-host-sandbox.md)

原文 L15–L20

~~~~~~~~text
**Status:** PROPOSED (2026-09-02). Written against a proposal that ADR-0078 is not enough to run a
local LLM in practice, and that ten provenance layers plus five security ADRs are missing beneath
it. The reading is right about the shape and wrong about the inventory: **nine of the ten
provenance layers already exist in this tree under other names** (§7 says where, field by field),
one is refused because it contradicts a decision this lineage has already paid for, and the five
security ADRs are **one** ADR — this one — because they are one rule.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L140: 3. Decisions / L408: Security amendment (2026-09-02) — corrections found reading the ADR against the tree / L462: Mission alignment amendment — 2026-10-07

<a id="source-0080-the-answer-is-long-the-verified-unit-is-short"></a>

## 0080-the-answer-is-long-the-verified-unit-is-short.md

[原文](../adr/0080-the-answer-is-long-the-verified-unit-is-short.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** Superseded in part — 分割 claim 案は 2026-09-03 に撤回。
~~~~~~~~

<a id="source-0081-long-context-the-input-is-a-state-chain"></a>

## 0081-long-context-the-input-is-a-state-chain.md

[原文](../adr/0081-long-context-the-input-is-a-state-chain.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** Superseded in part — prefill 分割案は 2026-09-03 に撤回。
~~~~~~~~

<a id="source-0082-the-close-is-flat-in-the-context"></a>

## 0082-the-close-is-flat-in-the-context.md

[原文](../adr/0082-the-close-is-flat-in-the-context.md) / [新しい主担当](0040-tir-terminal-proof-and-local-opening.md)

原文 L15–L44

~~~~~~~~text
**Status:** IMPLEMENTED (2026-09-04) on `palw-adr0082-impl`, pending the testnet-11 Relaunch 5f
cut. **§10 is the implementation record**, and it is where every figure below that the measurement
MOVED is corrected in place — struck, restated, with the configuration and the stream that moved
it. ~~PROPOSED (2026-09-03).~~ Written against two things: the refutation of ADR-0080 and
ADR-0081 (their own status blocks, verified line by line against the tree), and the state of
`palw-testnet-5f` at `0c299e7a` read beside `palw-adr0080-verification-segment` at `ee1a7582`.
The goal it serves is the one ADR-0077 R0 states and ADR-0080/0081 tried to reach: a person types a
prompt of thousands of tokens, receives an answer of thousands of tokens at the runtime's own speed,
and the chain holds ONE claim whose verification costs a bounded number of bytes wherever bytes are
spent and a bounded amount of compute wherever compute cannot be avoided. That is the shape Ambient
reached with a tolerant proof model (ADR-0026's audit); this ADR reaches it with the exact one.
Nothing here is armed by this document. Every consensus change below is a ruleset move and lands
with a re-genesis on testnet-11, as every relaunch has; mainnet ships PALW off and is untouched.
**Builds on:** ADR-0026 (borrow Ambient's architecture, refuse its proof model), ADR-0028 (sampling
decides nothing; only the court convicts), ADR-0030 §3 (the checkpoint leg and its state chunk map),
ADR-0040 Decision E (exact integer accumulation: the order of accumulation cannot change the
result — the associativity this ADR spends), ADR-0044 F4–F6 (beacon, panel, ticket), ADR-0049
Decisions C/E/F (admission bounds the court from the graph; the tiled logits pin; one description
for engine, profile, adjudicator and inventory), ADR-0072 (the ticket is the execution), ADR-0074
Decision 5 (a quantum is a fraction of the canonical job's leaves), ADR-0075 (a class is seated by
`ClassLaneCertified`), ADR-0077 (R0, R1, Decision 8's sampled interval, Decisions 10–14, SA-4's
derived deadline), ADR-0078 (what was made from the answer is committed, never carried), ADR-0080 §1
and Decision 3 (the measurement and the economic invariant — the two things that survived), ADR-0081
§1.2 and Decision 3 (the prompt ids as a Merkle root — the one decision that survived), and ADR-0080
"design A" as landed on `palw-testnet-5f` (the chunk-group close ceiling).
**Amends:** ADR-0080 §3 and ADR-0081 §3 — withdrawn and replaced (§7). ADR-0077 Decision 8 (the seat
no longer fetches the history), Decision 11 (attention is no longer priced by an interval), Decision
12 (the ladder's depth stops binding), Decision 13 (the rows are sized by three derived bounds, not by
a rotary table). ADR-0080 design A (the chunk count is re-derived from the flat terms, and its
unlanded half on 5f is named as a prerequisite).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L237: 3. Decisions / L799: 10.2 Decision 4 amended: a checkpoint at every position, and one route refused / L826: 10.3 What the capture costs — the half Decision 4 asserted and did not price / L947: 10.7 Status, and what is open — by name / L995: Mission alignment amendment — 2026-10-07

<a id="source-0083-the-difficulty-window-counts-only-rows-priced-by-bits"></a>

## 0083-the-difficulty-window-counts-only-rows-priced-by-bits.md

[原文](../adr/0083-the-difficulty-window-counts-only-rows-priced-by-bits.md) / [新しい主担当](0008-daa-difficulty-and-consensus-clock.md)

原文 L12–L23

~~~~~~~~text
**Status:** PROPOSED (2026-09-04), implemented on `palw-daa-bits-priced-rows` with the fence
dormant on every preset; **not armed anywhere**. How it reaches testnet-11 — a scheduled
`ForkActivation` on the live chain (fingerprint moves, genesis does not; a flag-day restart of the
seats, no wipe) or folded into a Relaunch 5g re-mint — is the operator's decision, recorded in §5.
**Builds on:** ADR-0060 Decision 1 (the heartbeat lane), ADR-0066 Decisions 1–3 (the lane's price
leaves `header.bits`; its slot rule; attempt blue work is a constant), ADR-0071 (`bits` keeps the
block-interval control that target-freezing removed), ADR-0072 (one inference, one draw: both the
class ticket and the Layer-0 digest are functions of the execution commitment).
**Amends:** ADR-0066 Decision 1, one sentence — *"Heartbeat headers carry the global expected bits
like every other lane, so they enter the difficulty window as ordinary rows and F1 and F3b both
disappear."* They carry the bits and they enter the window; they must not be **counted** as rows the
retarget slows down. F1 did not disappear; it moved from the lane's price to the lane's count.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L65: 4. Decision 1 — count only bits-priced rows; an empty count answers MAX / L98: 5. How it ships — the operator's decision

<a id="source-0084-the-ids-ride-the-capture-stays-home"></a>

## 0084-the-ids-ride-the-capture-stays-home.md

[原文](../adr/0084-the-ids-ride-the-capture-stays-home.md) / [新しい主担当](0025-context-and-input-commitments.md)

原文 L15–L31

~~~~~~~~text
**Status:** PROPOSED (2026-09-04), implemented on `palw-adr0084-served-answer` (see §7 for what
landed and what did not). **Consensus-inert**: no fence, no state field, no object variant; the
fingerprint does not move. Ships by a rolling rebuild of the seats and of every node that answers
material pulls; an old seat and a new executor interoperate (an old seat still pulls the whole
material and still gets nothing over the cap, exactly as today).
**Builds on:** ADR-0042 Decision 7 (the transport half: broadcast + pull), ADR-0073 Phase ① 1a/1e
(`FPC1` "the answer beside the question"; a seat checks, it does not re-run), ADR-0077 Decision 8
and SA-2 (the seat opens `k` intervals; "the executor never ships the capture whole"), ADR-0078 X6
(`output_root` is recomputable from the answer's ids, the job's context hash and the family's
rendered-hash rule), ADR-0082 Decisions 7 and 9 (the capture is a fold; a seat recomputes the
cache from the prompt it holds and "the committed output ids"), ADR-0065 Decision 4 (an
`Unavailable` quorum abstains — a separate fence, not touched here).
**Amends:** ADR-0073 Phase ① 1a, on the served-payload side only — what a seat is served for a
claim whose capture does not fit the transport is the question and the answer's ids, not the
capture. ADR-0077 Decision 8's last sentence — the attempt lane's seats "keep hashing their
canonical-job captures until Decision 14 makes those captures large, at which point the same arm
serves both lanes": the captures are large now, and this is that point.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L97: 3. Decisions / L342: 7.3 Decision 7 on the devnet (run 5, 23:22 JST, 3c6ea28b) / L463: Mission alignment amendment — 2026-10-07

<a id="source-0085-the-close-is-assembled-from-what-was-served"></a>

## 0085-the-close-is-assembled-from-what-was-served.md

[原文](../adr/0085-the-close-is-assembled-from-what-was-served.md) / [新しい主担当](0040-tir-terminal-proof-and-local-opening.md)

原文 L15–L24

~~~~~~~~text
**Status:** PROPOSED (2026-09-04). §6 items 1–3 LANDED 2026-09-04 (see §7); items 4–5 are being built under the 2026-09-05 goal, with ADR-0086 Decision 6's transport (the block-leaves annex) riding the same lane. Consensus-inert
by construction: the refutation object (`PalwExecutionStepRefutationV1`), the adjudicator
(`check_execution_step_refutation_v1`) and every court object are unchanged; what changes is how a
party that holds no capture ASSEMBLES the close, and what the executor serves it.
**Builds on:** ADR-0084 (the seat needs the answer's ids, never the history; the material lane
serves the envelope for a capture over the cap), ADR-0077 Decision 8 / SA-2 (the interval lane,
authenticated), ADR-0082 Decisions 4, 7 and 9 (the tile-addressed bottom; the fold; the seat
recomputes the cache), ADR-0073 Decision 1c (a free prompt is carried into the close), audit3 S-01
(the close is assembled from the ACCUSED's bytes by whichever party makes it).
**Amends:** nothing. ADR-0084 §8 named this as U-07c and left it undecided; this decides it.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L67: 3. Decisions / L208: Mission alignment amendment — 2026-10-07

<a id="source-0086-the-opening-carries-the-fold-not-the-leaves"></a>

## 0086-the-opening-carries-the-fold-not-the-leaves.md

[原文](../adr/0086-the-opening-carries-the-fold-not-the-leaves.md) / [新しい主担当](0040-tir-terminal-proof-and-local-opening.md)

原文 L17–L21

~~~~~~~~text
**Status:** PROPOSED 2026-09-05, implemented consensus-inert on `palw-adr0084-served-answer`
(no fence; the fingerprint does not move). Supersedes the wire form of ADR-0077 Decision 8's
interval opening (V1–V3) for SERVING; the consensus root rule those openings walk
(`step_range_opening_root_capped_v1`) is untouched. Requested by the operator on 2026-09-05 as
ADR-0084 §7.2's first option.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L59: 3. Decisions / L301: Mission alignment amendment — 2026-10-07

<a id="source-0087-a-position-is-bought-from-the-curve-and-sold-back-to-it"></a>

## 0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md

[原文](../adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) / [新しい主担当](0021-positions-and-membership.md)

原文 L18–L25

~~~~~~~~text
**Status:** PROPOSED 2026-09-05, design only (no implementation yet). Requested by the operator
on 2026-09-05: "Model Positions" — a per-model, fixed-supply position whose price is set by
market participants' beliefs about the model's future (its usage, its PALW work, its evaluation,
its migration to new versions, the scarcity of its capacity), traded against BILI on an AMM, with
1 % of every trade to the model's registrant and 5 % of every trade burned, and with **no
transfer between holders**, so that a position is never something one person hands another.
The operator's word is *position* (ポジション), not *share*: it is bought from the protocol's
curve and sold back to it, and that is the whole of what it is.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L12: 2026-10-09 改定 — Positionと市場の対象を固定する / L171: 3. Decisions / L371: 10. Amendment 2026-09-23: the holder id is not a payout payload (P-B4)

<a id="source-0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing"></a>

## 0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md

[原文](../adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) / [新しい主担当](0016-artifact-ownership-and-model-lineage.md)

原文 L20–L48

~~~~~~~~text
**Status:** PROPOSED 2026-09-05, **revised the same day** — design only (no implementation yet).
The first draft of this number (commit `4f198f6e`, "the class keeps its graph, and the exam
names its weights") made the successor of a model's weights the verdict of an on-chain exam:
candidates proposed by anyone, questions drawn by the beacon from a permissionless pool of pure
programs, answers tried by the court, a chained index of accepted improvements. The operator
withdrew that direction on 2026-09-05: *strengthening a model is one developer's work, not a
protocol's* — "分散モデル学習ではなく分散モデル市場" (not distributed training, a distributed
market). A Qwen-class 27B model is trained by one team; what the chain should do is record every
version that team publishes, keep the old ones, count how each is used, and let the market price
the line. This text replaces the first draft under the same number, renamed to say what it now decides
(README §"Number hygiene"). The draft's reasoning is summarised in §9 so it can be found again.
**Requested by the operator:** one registered address is one model-developing entity (1 アドレス =
1 モデル開発主体); that address alone updates the model, V1 → V2 → V3, continuously; every version
is recorded in a Model Registry that is "a Git for models" (weights hash, runtime hash, dataset
commitment, training config, evaluations, inference statistics, position, price); users use the
model and PALW processes the use; the position is priced on the MISAKA EVM's Model AMM; a line
that the market rates highly has the strongest reason to keep improving; and the design should
be the one that makes strengthening *easiest* (より強化しやすい設計).
**Builds on:** ADR-0087 (positions on a curve), ADR-0056 Decisions 3, 5, 6 and 7 (a class IS its
graph; the registrant's bond; duplicates; what the chain does not judge), ADR-0067 (classes are
chain data, kernels are the build), ADR-0069 / ADR-0075 (certification is of kernels, never of
weights), ADR-0059 (carve, never mint), ADR-0065 (a bond is the chain's address-shaped identity
for a PALW actor).
**Amends:** ADR-0087 Decision 1 (the market is keyed by a *line*, of which the class's own is the
first — byte-identical for every class that has only its own), Decision 4 (the registrant leg is
the line's *owner's*, and the owner may share it with an adopted contributor), Decision 7 ("a new
version is a new class" is narrowed to a new *graph*; new weights are a new *version* of a line,
and the market stays); ADR-0056 Decision 6's admission clause ("an attempt whose `artifact_root`
differs is admission-rejected") becomes "differs from every root in force for the class".
~~~~~~~~

判断・改定等を含む見出し（抽出）: L194: 3. Decisions / L483: 9. What the first draft decided, and why it was withdrawn

<a id="source-0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand"></a>

## 0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md

[原文](../adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) / [新しい主担当](0007-evm-exec-lane-and-anchors.md)

原文 L24–L39

~~~~~~~~text
**Status:** PROPOSED 2026-09-05, design only (no implementation yet). Requested by the operator
on 2026-09-05, on top of ADR-0087 and ADR-0088: Model Positions are not to be "a mere copy of
ERC-20" (ERC-20 の単なるコピーにせず); `ModelPosition`, `ModelRegistry` and `ModelAMM` are to be
**native primitives of the MISAKA EVM** — the thing that is MISAKA's own reason to exist
(Misaka 独自の存在意義); the current ERC is to be reshaped into an EVM design specialised for
trading model positions; and Hyperliquid's HyperEVM is the reference.
**Builds on:** ADR-0020 (the selected-parent EVM lane — live on testnet-10, testnet-11 and
devnet from genesis, inert on mainnet), ADR-0022 (the EVM snapshot at the pruning point),
ADR-0023 (Lane 2; per-lane precompile profile; the frozen security labels), ADR-0087 (the market
is a balance in the state fold; implemented behind `palw_model_market`), ADR-0088 (lines, versions,
roles, usage; the market keyed by line), ADR-0059 (carve, never mint), `docs/misaka-evm-design-v0.4.md` §3.2 (system
ops), §8 (fees), §9 (the UTXO ⇄ EVM bridge), §17 (invariants).
**Amends:** ADR-0087 Decision 3 (two more ways to make the same two moves — from the EVM),
Decision 8 (what a participant reads gains the EVM's surfaces); design v0.4 §9.1's supply
invariant (it names the market's sinks and the writer's escrow). ADR-0087 Decision 5 (no
transfer) is not amended: this ADR is the proof that the EVM cannot manufacture one.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L12: 2026-10-09 改定 — Positionと市場の対象を固定する / L177: 4. Decisions

<a id="source-0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole"></a>

## 0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md

[原文](../adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) / [新しい主担当](0022-liquidity-seeding-and-virtual-reserves.md)

原文 L18–L23

~~~~~~~~text
* Status: PROPOSED 2026-09-05; IMPLEMENTED 2026-09-05 on `palw-adr0088-0089-impl` (§8)
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decisions 1, 2,
  3 (a third move), 4 (no leg on the seed) and 8; [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
  Decisions 3, 5 and 6 (a third action, a third event).
* Builds on: 0087 (the market), 0088 (the line the market is keyed by), 0089 (the EVM's hand).
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L12: 2026-10-09 改定 — Positionと市場の対象を固定する / L61: 3. Decisions

<a id="source-0091-the-reward-buys-the-pair-and-no-holder-is-paid"></a>

## 0091-the-reward-buys-the-pair-and-no-holder-is-paid.md

[原文](../adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) / [新しい主担当](0023-model-reward-routing-and-owner-leg.md)

原文 L18–L32

~~~~~~~~text
* Status: PROPOSED 2026-09-06 (design first, at the operator's word; the implementation follows
  on `palw-adr0088-0089-impl` and is recorded in §8 when it lands)
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decision 3 (a
  move that is the chain's own, on no lane), Decision 4 (no leg on it) and Decision 8 (two more
  words on the row); [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)
  §10's open item "a per-line service fee … a share of a claim's escrowed reward to the owner"
  (decided: no — the reward's share goes to the pair, not to a person); [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
  Decision 2 (the AMM window's `market()` gains two words); [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decision 2 (the reserve now grows by the reward as well as by buys) and §5's conservation
  identity (a third source of BILI).
* Builds on: [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 10 (the worker reward is a
  carve of the subsidy, ESCROWED at the accepting block and named as a payout at `Final`, never
  minted before), 0088 Decision 4 (a claim is attributed to the version whose root it named),
  0090 (the seeded pair, the whole position, the product that never falls).
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L12: 2026-10-09 改定 — Positionと市場の対象を固定する / L85: 3. Decisions

<a id="source-0092-the-ladder-is-minted-once-and-the-clock-is-what-binds"></a>

## 0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md

[原文](../adr/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) / [新しい主担当](0018-long-context-measurement-and-admission.md)

原文 L15–L25

~~~~~~~~text
* Status: ACCEPTED 2026-09-06 (design first, at the operator's word; §8 records the implementation)
* Builds on: [0049](0049-palw-adjudication-contract.md) Decision C (what one round may cost),
  [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) (adjudicability is the price of
  weight), [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) Decision 12
  (the `2^32` context ladder), [0080](0080-the-answer-is-long-the-verified-unit-is-short.md) (the
  verified unit is short), [0082](0082-the-close-is-flat-in-the-context.md) Decisions 1, 3, 6 and 9
  (the k-ary court, the close ceiling, the seat's window),
  [0084](0084-the-ids-ride-the-capture-stays-home.md) U-08 (the walkers take the ruleset's ladder).
* Amends: nothing yet — §4 proposes amendments to 0082 Decision 3 only if Decision 3 of this ADR
  is taken.
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L116: 4. Decisions / L289: Mission alignment amendment — 2026-10-07

<a id="source-0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing"></a>

## 0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md

[原文](../adr/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) / [新しい主担当](0040-tir-terminal-proof-and-local-opening.md)

原文 L15–L31

~~~~~~~~text
* Status: PROPOSED 2026-09-06 (design only). **§6 steps 2–4 IMPLEMENTED 2026-09-11, together**
  (§9): both A16 families answer a fused dissection — the root claim, every round, the
  challenger's choice and the bottom's close — with every number computed by the court's own
  kernels, drilled on real captures before anything relies on it (§7's condition). Step 5 is
  settled by a fact the design did not have: the exemption it would narrow was never armed on any
  preset, and is now pinned unarmed. Step 6 — the fleet upgrade that makes testnet-11's dense
  graph-v5 producers defendable — is the operator's. **Amended 2026-09-11 (§10):** Decision 6
  (admission refuses an undissectable fused class past its own fence, `None` everywhere) and
  Decision 7 (a forged FOLD is bottomed from the challenger's honest prefix and the root claim's
  own tile — no row served by the accused) are built; §9's straddling finding is corrected.
* Builds on: [0082](0082-the-close-is-flat-in-the-context.md) Decisions 2 and 3 (the k-ary history
  dissection and its arity), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md)
  (adjudicability is the price of weight),
  [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) §7 (which names this as the
  one thing it deliberately did not decide).
* Amends: nothing. It supplies the half of ADR-0082 Decision 2 that was specified and never built.
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L86: 3. Decisions / L300: 10. Amended 2026-09-11: Decisions 6 and 7 / L402: Mission alignment amendment — 2026-10-07

<a id="source-0094-a-seed-is-paid-in-as-many-transactions-as-it-takes"></a>

## 0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md

[原文](../adr/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) / [新しい主担当](0022-liquidity-seeding-and-virtual-reserves.md)

原文 L14–L19

~~~~~~~~text
* Status: PROPOSED 2026-09-07; IMPLEMENTED 2026-09-07 (§8)
* Amends: [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decision 2's last clause ("One seed a line: a second is refused") and Decision 5's row.
* Builds on: 0090 (the seed IS the reserve and never leaves), 0087 (the curve), 0091 (the reward
  buys the pair).
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L75: 3. Decisions / L191: Amendment (2026-09-07): this needed a fence, and its field needed to stay derivable

<a id="source-0095-a-position-is-a-membership-not-an-income"></a>

## 0095-a-position-is-a-membership-not-an-income.md

[原文](../adr/0095-a-position-is-a-membership-not-an-income.md) / [新しい主担当](0021-positions-and-membership.md)

原文 L20–L40

~~~~~~~~text
* Status: PROPOSED 2026-09-07
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decision 1's
  "grants nothing but the right to sell it back" — it now also grants what its LINE declares;
  [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) Decision 2 (a version's
  entry and promotion are constrained by the line's declaration) and Decision 12 (rows to read).
* **Amended 2026-10-04 (the operator): no served inference — every job is self-ordered.** MISAKA has no
  hosted or third-party inference service: a user submits their OWN job, the network's seats verify it, and
  nobody rents GPUs to run other people's jobs or offers an API on the chain's behalf
  ([ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P1: third-party serving is a
  non-goal). So the grants that describe a serving queue — bit 2 `PRIORITY_INFERENCE` ("the line's gateways
  serve holders' jobs ahead of others'") and bit 5 `INFERENCE_QUOTA` ("served capacity") — promise something
  that does not exist and are **retired**: no surface may present them as a benefit and a line should not
  declare them. (The consensus-inert code in `palw_model_benefits_v1` / `palw_service_descriptor_v1` still
  accepts the bits; refusing them at the fold is a later fence.) Likewise "gateway" below means the holder's
  OWN entrance (ADR-0096) or a service the line's owner runs off-chain — not a protocol service. What a Position
  can protocol-natively unlock in future is **holder-only job classes** (job admission reading the holding —
  e.g. a class whose jobs require `PositionRequired`); today no job admission path reads a Position, so a
  Position is an on-chain membership card whose other benefits are the owner's to provide.
* Builds on: 0087 (no transfer, the curve), 0088 (lines, versions, previews, evaluations,
  proposals, roles), 0090 (the seeded pair), 0091 (the reward buys the pair — income was already
  refused).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L12: 2026-10-09 改定 — 固定モデルに対するmembership / L88: 3. The boundary, and what is on each side of it after this revision / L111: 4. Decisions / L152: 4.4 The lead is a consensus rule, not a promise — this is the revision's core

<a id="source-0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed"></a>

## 0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md

[原文](../adr/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md) / [新しい主担当](0024-inference-input-output-contract.md)

原文 L15–L36

~~~~~~~~text
* Status: PROPOSED 2026-09-10. Part A (the entrance) and Part C (distribution, settings, the
  model-request intake) are host-side and land on `feat/adr-0096-everyday-lane` in both
  repositories (misakas and MISAKA-Studio) as they are built — §9 records what has. Part B (the
  shape of the answer) is a ruleset move behind a fence that is `None` on every preset and, as
  ADR-0082 Decision 11 did, is REFUSED at assembly on a build that cannot carry it; nothing in
  Part B changes a shipped job, a shipped claim or the fingerprint.
* Builds on: [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) (R0: one
  inference, one claim; Decision 2: the answer streams, the commitment does not; Decision 3: the
  gateway reads the chain it commits to; Decision 6: the template speaks the model's own control
  tokens; Decision 13: the 512-token rows; Decision 16: `PanelDa`),
  [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) (Decision 2: the DSL is
  the claim's output, canonicalized by a registered grammar; Decision 6: delivery; Decisions 8 and
  9: the kind table is open), [0082](0082-the-close-is-flat-in-the-context.md) (Decision 11: the
  seeded argmax, and the one fence that arms the sampler; §Part D),
  [0084](0084-the-ids-ride-the-capture-stays-home.md) / [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  (what is served, and its cap), [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md)
  (Decision 4: a wider row is a new class at mint, never a raised ceiling),
  [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) (a line, an owner, a
  version — the thing a model request becomes).
* Amends: ADR-0078 Decision 6's rendering premise, by Decision 6 below, **only past the fence of
  Decision 8** — the shipped rendering does not move.
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L182: 3. Decisions / L680: Mission alignment amendment — 2026-10-07

<a id="source-0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token"></a>

## 0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md

[原文](../adr/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) / [新しい主担当](0018-long-context-measurement-and-admission.md)

原文 L15–L35

~~~~~~~~text
* Status: PROPOSED 2026-09-10; **IMPLEMENTED the same day, consensus-inert** (§9). No fence, no
  field of `Params`, no object, no fingerprint moves: Decision 1 is a report over predicates the
  chain already runs, Decision 2 is two JSON objects on the gateway, and Decisions 3–5 are what
  the report says about the question that prompted it.
* Builds on: [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) (Decisions 1–4:
  the ladder is minted once, the window binds, the arity is derived; §5: every number is a
  generated artifact), [0082](0082-the-close-is-flat-in-the-context.md) (Z4: the window gate;
  Decision 3: the arity; Decision 9: the seat's window is the drill's),
  [0081](0081-long-context-the-input-is-a-state-chain.md) (§1.1: the hybrid is the structurally
  better long-context candidate; §1.2: the ids are the linear term; Decision 3: the tiled root),
  [0080](0080-the-answer-is-long-the-verified-unit-is-short.md) (§1: the residue is the width),
  [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) (Decisions 12–14: the
  ladder, the rows, the canonical job; Decision 16: `PanelDa`),
  [0096](0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)
  (Decision 1: the surface; Decision 5: a long thread is a chain of jobs; Decision 8: the token
  table as served material; Decision 10: the manifest), [0084](0084-the-ids-ride-the-capture-stays-home.md) /
  [0086](0086-the-opening-carries-the-fold-not-the-leaves.md) (what a seat is served),
  [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) (the
  responder), [0079](0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md)
  (the host).
* Amends: nothing. Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L207: 3. Decisions / L458: Mission alignment amendment — 2026-10-07

<a id="source-0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else"></a>

## 0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md

[原文](../adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md) / [新しい主担当](0031-panel-independence-and-unavailability.md)

原文 L15–L37

~~~~~~~~text
* Status: PROPOSED 2026-09-10. **Decisions 1–3 IMPLEMENTED the same day** (§9): Decision 1 is a
  report over the draw the seats run; Decisions 2 and 3 change what a seat FILES, which is the
  seat's own duty — no consensus object, acceptance rule, fence, parameter or fingerprint moves,
  and a fleet takes them by an ordinary rolling rebuild. Decisions 4–6 are stated and not built.
* Builds on: [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) Decision 8
  (a seat replays `k` intervals drawn from the beacon; "a row unequal: the seat files nothing and
  opens a court at that leaf"; a sampled verdict convicts nobody),
  [0082](0082-the-close-is-flat-in-the-context.md) Decision 9 (the seat recomputes the state it
  resumes from), [0085](0085-the-close-is-assembled-from-what-was-served.md) Decision 4 (a seat's
  fault is prosecuted by the challenger's half), [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  Decision 6 (a leaf once named), ADR-0065 Decision 4 (`Unavailable` abstains),
  [0080](0080-the-answer-is-long-the-verified-unit-is-short.md) (a receipt is 4,772 bytes on the
  wire), [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) Decision 4,
  [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) (the missing
  responder), [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)
  Decision 5 (whose first row — a seat that holds a shard, measured first — this is).
* Answers: ADR-0081 Decision 8's question and its U-03 — "what fraction of a job's segments the
  panel collectively covers per claim and what a producer's expected cost is of a single forged
  link" — which ADR-0082 withdrew with ADR-0081 §3 before anyone took the number. The question was
  never about segments; it is about ADR-0077 Decision 8's live draw, and it is answered there.
* Amends nothing in consensus. **Restores** ADR-0077 Decision 8's "the seat files nothing" in the
  node, where three paths had lost it (§1.3).
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L170: 3. Decisions / L318: Mission alignment amendment — 2026-10-07

<a id="source-0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard"></a>

## 0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md

[原文](../adr/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) / [新しい主担当](0030-sharded-verification-and-model-measurement.md)

原文 L15–L42

~~~~~~~~text
* Status: PROPOSED 2026-09-10. **Decisions 1–4 and 6 IMPLEMENTED the same day, consensus-inert**
  (§9): types, pure functions, a generator and tests; no object, no acceptance rule, no fingerprint
  moves on any shipped preset. **Decision 5's fence is declared and refused at assembly** on this
  build, in the shape ADR-0096 Decision 8 used. Decisions 7–8 are stated, not built. Every
  MEASUREMENT this ADR needs is the ADDER's, taken by the tool this ADR ships, and is recorded as a
  unit the adder runs (§6), never as a number this document knows.
* Builds on: [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)
  (the fit as a lookup; Decision 5's first row, which this ADR is; §1.3's stand-in, which this
  ADR turns into a manifest), [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
  (the coverage of a one-leaf lie is independent of the shard count; licensing fits one
  transaction up to eight shards; a seat that found a lie files nothing; the court a shard can
  open, named there and built here as far as a type), [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md)
  Decision 8 (a seat opens a court at the leaf, holding the refutation's inputs),
  [0082](0082-the-close-is-flat-in-the-context.md) Decisions 4 and 9 (the cache half of the
  ladder is court material, tile-addressed; the seat recomputes its state from the prompt it
  holds, and the drill enforces its window),
  [0075](0075-certification-is-a-consensus-object.md) (a class is seated by a certification the
  chain's own drill produces), [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) SA-1
  (a bond declares what it can judge), [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md)
  Decision 4 (a wider row is a new mint), [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md)
  (the fused site's terminal), [0096](0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)
  Decisions 10 and 13 (the components manifest; a model request has a door),
  [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) / [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  (a line, an owner, a seeded pair — what a registered model becomes).
* Amends: ADR-0097 Decision 4 keeps its rule — a stand-in is a verdict, never a row — and gains
  a form: a stand-in is now also a MANIFEST (Decision 1), read by the same generator that will
  read a converted model's, and the day a model is converted its manifest is what names the
  class. Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L167: 3. Decisions / L363: Mission alignment amendment — 2026-10-07

<a id="source-0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard"></a>

## 0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md

[原文](../adr/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) / [新しい主担当](0015-permissionless-model-registration.md)

原文 L15–L39

~~~~~~~~text
* Status: PROPOSED 2026-09-10. **Decisions 1, 2, 3, 4 and 6 IMPLEMENTED the same day,
  consensus-inert on every shipped preset** (§9): the one-move court is a consensus object behind
  the fence ADR-0099 declared, `None` everywhere, and the fence arms only over a genesis that
  commits to the court's signing context; `palw-class measure` and `verify` exist and were run on
  the real A16 artifact; licensing per shard is a consensus object behind its own fence
  (`Params::palw_shard_licensing`), with no state-version move (Decision 4, amended the same day).
  **Decisions 5, 7 and 8 are stated, not built**, each with the reason. Every number below is the generator's or
  the tool's, recorded in `docs/palw-shard-plan-2026-09-10.md` and §1.3.
* Builds on: [0099](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)
  (Decision 5's fence and "the four"; §6 steps 2–4; Decision 4 named and not built),
  [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
  (a seat that found a lie files nothing else — now it files the one thing; licensing fits one
  transaction up to eight shards), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  (classes are chain data, kernels are the build; Decision 5's fenced interpreter and SA-4's
  "an unpaid panel is the cheapest thing on the chain to buy"; Decision 6's tiers and
  "registration and possession are different acts"), [0062](0062-data-availability-court.md)
  SA-1 (who may accuse, and at what price), [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md)
  / [0095](0095-a-position-is-a-membership-not-an-income.md) (a position is bought from the curve
  and sold back to it, transfers nothing and pays nobody), [0056](0056-palw-permissionless-class-admission-and-share-economy.md)
  (no allowlist, no vote, no identity at admission), [0075](0075-certification-is-a-consensus-object.md)
  (anyone certifies; the court grades), [0049](0049-palw-adjudication-contract.md)
  Decision G (the inventory is the layout an `artifact_root` commits to).
* Amends: ADR-0099 Decision 5 (built: the refusal "not a consensus object" is replaced by the
  ruleset-move condition), ADR-0099 §6 step 3 (`palw-class measure`, built), ADR-0099 Decision 4
  (licensing per shard: designed here, pure half built). Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L133: 3. Decisions / L417: Mission alignment amendment — 2026-10-07

<a id="source-0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves"></a>

## 0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md

[原文](../adr/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) / [新しい主担当](0021-positions-and-membership.md)

原文 L12–L34

~~~~~~~~text
* Status: PROPOSED 2026-09-10. **Decisions 2, 3 and 5 IMPLEMENTED the same day, consensus-inert**
  (§9): which grant needs whom, the signed service descriptor and the check a client runs
  against chain facts, and three pins that a Position moves only between a holder and the
  curve. Decisions 4 and 7 are stated, not built. Nothing moves on any preset: no rule verifies a
  descriptor, no fold reads one, no fence exists.
* **ADR-0144 alignment (2026-09-21).** Unimplemented D4 / D6 / D7 steps 2–4 (anyone-serves
  discovery and public serving) are withdrawn as PALW-reward work. See the end section.
* **2026-10-04 (the operator):** `PRIORITY_INFERENCE` and `INFERENCE_QUOTA` are retired (ADR-0095's amendment):
  every job is self-ordered and nobody serves inference to holders, so Decision 2's "any provider" row no longer
  includes them; a descriptor offering them describes nothing MISAKA has.
* Builds on: [0095](0095-a-position-is-a-membership-not-an-income.md) (a Position is a
  membership: the line declares, the chain computes the tier, a gateway reads it; §8's "a bond
  behind the promise" rejected; the reference check of §7 step 6),
  [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) (bought from the curve,
  sold back to it, no transfer), [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)
  (a line's owner, developer and maintainer; anyone founds a competing line),
  [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) (the EVM is the
  fold's window and hand, never a second market), [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
  (use reaches the curve; no holder is paid), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  Decision 6 (the chain carries kilobytes and no URL), [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the boundary "permissionless" means, of which this is the membership half).
* Amends: nothing. Supersedes nothing. Records, as a decision, what the operator settled on
  2026-09-10: **a Position has no payment or settlement role, and therefore no transfer.**
~~~~~~~~

判断・改定等を含む見出し（抽出）: L89: 3. Decisions

<a id="source-0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity"></a>

## 0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md

[原文](../adr/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md) / [新しい主担当](0014-versioned-kernels-and-backends.md)

原文 L12–L29

~~~~~~~~text
* Status: PROPOSED 2026-09-10 on `feat/adr-0099-sharded-seat`; **Decisions 1–4 IMPLEMENTED the same
  day, consensus-inert on every shipped preset** (§9): the graph, the kernel, its fence, the
  admission refusal, the registration root, the manifest variant and the measurement. The fence
  `Params::palw_token_lift` is `None` everywhere and the kernel is outside `court_catalog_root`, so
  testnet-11's and devnet's fingerprints do not move (`shipped_presets_have_pinned_fingerprints`).
  Decision 5 (the table row, the drill family, the arming) is stated, not built.
* Builds on: [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the held measurement, whose §1.3 refused this artifact by name),
  [0082](0082-the-close-is-flat-in-the-context.md) (graph-v5's fused attention, which graph-v6
  keeps), [0070](0070-the-model-tiers-step-spaces-are-adjudicable.md) §7(b) (an inventory refuses a
  store it cannot serve rather than inventing one), [0042](0042-palw-mainnet-candidate-ruleset.md)
  Decision 11 and [0052](0052-palw-qwen36-hybrid-class.md) (`court_catalog_root` is part of every V2
  ruleset id), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) /
  [0075](0075-certification-is-a-consensus-object.md) (weight needs a certified family),
  [0067](0067-classes-are-chain-data-kernels-are-the-build.md) Decision 5 (the chain-registered arm),
  [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) (the fused
  site's responder, which no family implements yet).
* Amends: ADR-0100 §1.3 (its last row now has an answer). Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L118: 3. Decisions

<a id="source-0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm"></a>

## 0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md

[原文](../adr/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md) / [新しい主担当](0025-context-and-input-commitments.md)

原文 L15–L55

~~~~~~~~text
* Status: PROPOSED 2026-09-11 on `docs/adr-0103-the-context-is-held-off-the-chain` (from
  `feat/adr-0099-sharded-seat` at `3e9c3b59`); **IMPLEMENTED 2026-09-11 on
  `feat/adr-0103-held-context`** (§10: what each Decision became, the generator's numbers, ten
  corrections to the text below, and what is not done). Nothing is armed: the fence
  (`Params::palw_held_context`) is `None` on every shipped preset and Some-only in the fingerprint,
  and a network that wants what this ADR describes MINTS with it (ADR-0092 Decision 4;
  `palw_held_context_mint_v1`). Testnet-11 does not move. Every number in §1 is one the tree's generators already printed or an ADR already
  recorded, cited by section; every number in §3 that is not is labelled *arithmetic* and is the
  generator's to print before it is normative (ADR-0092 §5).
* Amended 2026-09-12 by [ADR-0118](0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md):
  the regime may also be armed at a HEIGHT on a network minted before it (testnet-11's flag day is
  one line, `PALW_RC_HELD_FENCE_DAA`, and the operator's). V4 and the tiled prompt ids are required
  at the fence only from genesis; past it the fence commits V4 itself and a held class commits its
  own Merkle ids. On such a network the frozen ladder, not the regime, bounds a held class's
  context (ADR-0118 §4).
* Builds on: [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)
  (the nine walls and their homes; §1.4: 2M is refused past eight layers; Decision 5's table, whose
  rows this ADR answers one by one), [0099](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)
  (a seat holds a shard; §1.2's two transfer forms, of which the *resume* form is Decision 2 here),
  [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the one-move court is a consensus object and convicts on a live devnet; licensing per shard),
  [0082](0082-the-close-is-flat-in-the-context.md) (R4; Decision 2's dissection; Decision 3's
  derived arity; Decision 4 as amended — a checkpoint at every position; Decision 7's fold;
  Decision 9's seat; §10.3's quadratic term; §8's two refusals this ADR reverses by name),
  [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) (the ladder is in the
  identity; the clock binds; a wider model is a new mint), [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  (an opening is the fold's digests and a frontier; the court's address is a block, then a leaf),
  [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
  (the coverage is a generated number; a seat that found a lie files the one thing),
  [0062](0062-data-availability-court.md) (an accusation names what is missing; a disclosure is
  hash-checked and bounded), [0081](0081-long-context-the-input-is-a-state-chain.md) Decision 3 /
  [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) Decisions 8 and 16
  (the tiled ids; the sampled interval; `PanelDa`), [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md)
  (the responder a fused site still needs), [0102](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)
  (a fenced kernel stays out of the identity — the shape every new thing here takes), and the
  close-cut ADR authored as 0102 on `feat/adr-0096-partb-drill` (§9: it renumbers) — a close too
  wide for one carrier is cut once, which is what carries a one-move court's object when it is wide.
* Amends: ADR-0082 R4, Decisions 3 and 9 and §8 (for a class under this fence only — §7);
  ADR-0092 Decision 1 (what a ladder is minted against, once it costs no rounds); ADR-0097
  Decision 3 (the geometry ceiling's form); ADR-0062 Decision 1 (what an accusation may name);
  ADR-0099 §1.2 (the resume form is chosen, for wide classes, by derivation). Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L194: 3. Decisions / L526: 10.1 What each Decision became / L693: 10.6 Decision 2's price, amended (2026-09-11): a replay pays for its history / L799: Mission alignment amendment — 2026-10-07

<a id="source-0105-a-heartbeat-never-turns-a-bonded-block-red"></a>

## 0105-a-heartbeat-never-turns-a-bonded-block-red.md

[原文](../adr/0105-a-heartbeat-never-turns-a-bonded-block-red.md) / [新しい主担当](0009-heartbeat-floor-and-liveness.md)

原文 L12–L32

~~~~~~~~text
* Status: PROPOSED 2026-09-11 on `fix/heartbeat-trap-slow-producers` (from `main` at `a5f1bdf7`).
  **Decision 1 IMPLEMENTED behind `Params::palw_heartbeat_transparent`, `None` on every shipped
  preset** — no fingerprint, identity, schedule or fork id moves on any network, and arming it on
  testnet-11 is a flag day (§7). **Decision 2 IMPLEMENTED in the heartbeat miner** — node policy,
  no rule, safe to roll out host by host today. **Decision 3** is operator guidance, written into
  [`testnet11-node-operator.md`](../history/networks/testnet-11/testnet11-node-operator.md) §7a.
* **Amended 2026-09-25 — F1 (§11), a post-launch flag day**: past
  `Params::palw_heartbeat_transparent_same_chain` transparency stops at the merging block's own
  selected chain; the rule as written (and as testnet-12 launched with it, from genesis) lets a
  private heartbeat branch borrow the public chain's attempts. The fence is `None` on every shipped
  preset, testnet-12 included, until an operator schedules it.
* Builds on: [ADR-0060](0060-the-liveness-doctrine.md) Decisions 1–2 (the heartbeat lane and its
  ramp), [ADR-0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
  Decisions 1–3 (the lane's own id and constant price; the one-block-deep slot rule; ε against the
  attempt lane's constant), [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)
  Phase 1 (`palw_attempt_work` = 2²⁰; the F3a width bound), [ADR-0058](0058-palw-merged-work-is-counted.md)
  (merged work — reds included — is applied to the PALW state), [ADR-0083](0083-the-difficulty-window-counts-only-rows-priced-by-bits.md).
* Amends: ADR-0066's "the ramp has two steps … the chain is producing, or it is not" — one sentence
  of it, the unstated premise that a producing chain produces faster than the recovery cadence.
  `heartbeat_interval_ms` and `check_heartbeat_slot` are unchanged.
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L168: 5. Decisions / L170: 5.1 Decision 1 (consensus, fenced) — a heartbeat never turns a bonded block red / L237: 5.2 Decision 2 (node policy, ships now) — the heartbeat miner steps aside for a landed draw / L268: 5.3 Decision 3 (operators) / L402: 11. Amendment F1 (2026-09-25): transparency stops at the merging block's own chain — a post-launch flag day

<a id="source-0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model"></a>

## 0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md

[原文](../adr/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md) / [新しい主担当](0017-model-streaming-and-memory-bounds.md)

原文 L15–L30

~~~~~~~~text
* Status: PROPOSED 2026-09-11 on `feat/adr-0099-sharded-seat`; **W1–W7 IMPLEMENTED the same day,
  consensus-inert** (§9): the leaf, its preimage, the tree, the layout rules and every opening are
  the ones the court already verifies — what changed is how a node BUILDS them, and every root,
  count, byte total, placement and document id it builds is byte-identical to the materializing
  builder's (§1.3, §1.4). No preset, fingerprint or rule moves. W8 is the ADDER's run, not an
  operator's (ADR-0099: the adder measures, the chain recomputes) — this ADR is what lets an
  adder's machine measure a model larger than its memory; measured here on the 2B, extrapolated
  for the 35B (§1.5). W9 and W10 are stated, not built.
* Builds on: [0049](0049-palw-adjudication-contract.md) Decision G (the canonical inventory and its layout
  rules), [0099](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) (the shard plan, which places inventory rows),
  [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the held measurement), [0102](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)
  Decision 3 (a graph-v6 registration pins the INVENTORY root — so building the inventory became
  part of registering and serving) and §8 (which named this ADR's subject as the K3 prerequisite).
* Amends: ADR-0102 §4 and §6 step 4 (the 33 GiB copy is gone from measurement and registration).
  Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L103: 3. Decisions / L242: Mission alignment amendment — 2026-10-07

<a id="source-0107-a-share-grows-on-work-that-reached-final"></a>

## 0107-a-share-grows-on-work-that-reached-final.md

[原文](../adr/0107-a-share-grows-on-work-that-reached-final.md) / [新しい主担当](0020-class-share-and-allocation-evolution.md)

原文 L23–L34

~~~~~~~~text
* Status: PROPOSED 2026-09-11 on `fix/share-growth-counts-final-work` (from `main` at `3c78e898`).
  **IMPLEMENTED behind `Params::palw_share_growth_final`, `None` on every shipped preset**: no
  fingerprint, identity, schedule or fork id moves on any network. Arming it on testnet-11 changes
  the share table that a boundary block writes into the PALW state root, so it is a flag day (§5).
* Builds on: [ADR-0054](0054-palw-share-follows-production.md) (a class's cadence share follows its
  own production), [ADR-0056](0056-palw-permissionless-class-admission-and-share-economy.md) (its
  attack table: "fake demand growth costs full production price"), [ADR-0045](0045-palw-class-economy-on-chain.md)
  Decision 2 (the per-class epoch budget), [ADR-0058](0058-palw-merged-work-is-counted.md) (merged
  blues and reds are applied to the PALW state).
* Amends: ADR-0054's growth condition — one clause of it (§2). The budget, the decay arm, the
  retarget and reclamation are unchanged.
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L1: ADR-0107 — A class's share grows on work that reached Final, not on blocks that were accepted / L69: 2. Decision / L163: Mission alignment amendment — 2026-10-07

<a id="source-0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote"></a>

## 0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md

[原文](../adr/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md) / [新しい主担当](0032-sampling-beacon-and-receipts.md)

原文 L23–L43

~~~~~~~~text
* Status: PROPOSED 2026-09-11, at the operator's request ("このような拡張をパーミッションレスで行える
  基盤を作成するADRを作成してから実装して"). **Decisions 1–9 IMPLEMENTED the same day** (§9),
  consensus-inert: no consensus object, acceptance rule, fence, parameter or fingerprint moves. A
  fleet takes it by an ordinary rebuild, and a node that never takes it is not partitioned from one
  that did.
* Builds on: [0046](0046-palw-v2-consensus-object-carriage.md) (an identity is derived, never
  declared), [0054](0054-palw-share-follows-production.md) / [0056](0056-palw-permissionless-class-admission-and-share-economy.md)
  (admission is a deterministic gate, not a vote), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  (a class the build never heard of is served from the registration the chain carries — behind a seal
  the operator arms), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) Decision 5 (a drill's
  evidence is readable by anyone), [0075](0075-certification-is-a-consensus-object.md)
  (certification is two lifecycle objects any transaction can carry), [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md)
  Decisions 3 and 5 and its SA-5 (a transformer is named by its manifest, a consumer recomputes, and a
  manifest nobody can fetch is no derivation), [0072](0072-the-ticket-is-the-execution.md) SA-2
  (the fork-id gate: a scheduled height is compared, never a claim about it), and the doctrine that
  consensus changes ship as an activation and never as a re-genesis.
* Answers the operator's 2026-09-11 assessment (§1): "the verification platform is mostly built; the
  extension platform is half built; what is missing is SOURCE → canonical form, shared across the
  kinds of thing a person can bring."
* Amends nothing in consensus. Adds one crate, one CLI verb with five forms, and one document format.
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L141: 3. Decisions / L143: Decision 1 — One manifest, canonical, with a derived identity / L189: Decision 2 — Every answer names its tier / L208: Decision 3 — Verification has three depths, and the report says which it reached / L226: Decision 4 — A receipt is evidence of reproduction, signed by whoever reproduced it — and it is not a vote / L249: Decision 5 — Preflight and submit go through the objects that already exist; nothing new rides / L270: Decision 6 — A ruleset candidate is described and costed; it is never activated by a manifest / L286: Decision 7 — A context width is a class inside the ladder and a ruleset candidate outside it / L298: Decision 8 — The chain-class seal stays; the manifest tells the operator it is there / L306: Decision 9 — Publication and discovery are off-chain, and the id is what makes that safe / L349: 6. Security amendments, stated before the build / L513: Mission alignment amendment — 2026-10-07

<a id="source-0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause"></a>

## 0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md

[原文](../adr/0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md) / [新しい主担当](0027-claim-lifecycle-and-liability.md)

原文 L17–L33

~~~~~~~~text
* Status: PROPOSED 2026-09-11, at the operator's request ("今の EVM ブリッジや送金がスムーズに liveness
  に行かない根本的原因を分析して liveness を優先にする設計に修正を行なって"). **Decisions 1–5
  IMPLEMENTED the same day** (§9), consensus-inert: no block-validity rule, no `Params` field, no
  fingerprint moves. A node takes it by an ordinary rebuild and restart; a node that never takes it
  validates every block the new nodes make.
* Builds on: [0020](0020-selected-parent-evm-lane.md) (the EVM lane executes on the selected chain;
  B's own payload is executed by its selected child), [0009](0009-dns-probabilistic-finality.md)
  (DNS finality is probabilistic and two-resource; "liveness depends on both PoW miners and PoS
  validators while the overlay is active"), [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md)
  (the 09-10 heartbeat trap, during which the bridge stood paused), [0072](0072-the-ticket-is-the-execution.md)
  (a draw is the execution; a template is fixed for the whole draw), the 2026-09-10 bridge-freshness
  fix (`dns_finality_fresh_for_bridge` measured in blue score beyond the anchor's structural
  distance), and ADR-0108's rule that a node extension is policy a build carries, never a rule the
  chain enforces.
* Amends: the producer policy and the RPC policy of kaspa-pq EVM Lane v0.4 §9.2 (deposit claims)
  and §15 (template assembly). The claim's *validity* (`validate_one_deposit_claim`) is unchanged.
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L125: 3. Decisions / L127: Decision 1 — Every accepted lock is claimed by every producer, unasked / L145: Decision 2 — Finality is a label the reader asks for, not a pause the producer imposes / L158: Decision 3 — A spend the chain will refuse is refused at the mempool / L166: Decision 4 — safe is the DNS-confirmed anchor / L174: Decision 5 — The claim RPC and the relay lane stay, as accelerators / L189: 5. Security amendments / L285: Mission alignment amendment — 2026-10-07

<a id="source-0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation"></a>

## 0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md

[原文](../adr/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md) / [新しい主担当](0018-long-context-measurement-and-admission.md)

原文 L15–L28

~~~~~~~~text
* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context`, written from the operator's decision
  of the same day (ADR-0103 §10.5 answered by ADR-0111; this is the other half of that answer).
  **Decisions 1–5 IMPLEMENTED the same day (§9)**; the 512, 4,096 and 32,768-position vectors pass
  every stage and are pinned. Consensus-inert: no object, acceptance rule, fence, parameter or
  fingerprint moves. A fleet takes it by an ordinary rebuild.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (§10.5: no 2M claim ran; the "done when" was met at devnet widths and at 2M only in the
  generators' tables), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) (the
  last prosecution gap before arming), [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) and [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) §5 (a
  figure is a generated artifact, and a figure an ADR prints that the generator does not is a bug in
  the document), and ADR-0108 on `feat/adr-0108-extension-envelope` (Decision 4: a receipt is
  evidence of reproduction and never a vote; Decision 6: a ruleset change is described and costed,
  and activated only by a release).
* Supersedes nothing. Amends nothing in consensus.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L64: 2. Decisions / L249: 9.1 Where each Decision lives / L387: Mission alignment amendment — 2026-10-07

<a id="source-0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge"></a>

## 0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md

[原文](../adr/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) / [新しい主担当](0039-data-availability-court-and-panel-da.md)

原文 L15–L33

~~~~~~~~text
* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context` (continuing ADR-0103 at `36f115c4`),
  written from the operator's decision on ADR-0103 §10.5; **IMPLEMENTED the same day (§8), both
  paths drilled to a slash on a live devnet**. Nothing is armed: the new unit rides the held regime
  (`Params::palw_held_context`, `None` on every shipped preset) and ADR-0062's court, so no shipped
  fingerprint, identity, schedule or fork id moves. Testnet-11 does not move.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (Decision 1: the one-move court is the court; §10.3 item 11: the prompt tile rides the accusation;
  §10.5: the gap this ADR closes), [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the one-move court and its false-accusation charge), [0062](0062-data-availability-court.md)
  (an accusation names what is missing; SA-2 an answer is hash arithmetic; SA-4 a refuted accuser
  pays; Decision 5 silence is the default), [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md)
  Decision 8 and SA-2 (the seat's draw; the authenticated interval lane),
  [0086](0086-the-opening-carries-the-fold-not-the-leaves.md) Decision 6 (a block's leaves name the
  leaf), 0085 §6 item 4 (the close annex: the committed tiles of a disputed leaf),
  [0081](0081-long-context-the-input-is-a-state-chain.md) Decision 3 (the prompt's tiled root).
* Amends: ADR-0103 Decision 1 (who holds a refutation's inputs) and §10.3 item 8 (the
  data-availability court is required at the fence after all — §2 Decision 5); ADR-0062 Decision 1
  (what an accusation may name: a leaf's committed evidence); ADR-0085 §6 item 4 (the annex is
  served for a leaf a seat names, not only for a leaf an open session names). Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L65: 2. Decisions / L214: 8.1 What each Decision became / L302: Mission alignment amendment — 2026-10-07

<a id="source-0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact"></a>

## 0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md

[原文](../adr/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md) / [新しい主担当](0017-model-streaming-and-memory-bounds.md)

原文 L15–L32

~~~~~~~~text
* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context`, from the operator's decision of the
  same day after the testnet-11 producers were measured (§1): "the same model and the same weights,
  with the way they are run changed from resident-whole to explicit streaming; the formal target is
  a model five times the memory available to it". **Decisions 1–5 and 7–8 IMPLEMENTED the same day
  (§10)**; Decision 6's fleet measurement waits on the operator's go, because it runs a new binary
  on a fleet host. Consensus-inert: no object, acceptance rule, fence, parameter or fingerprint
  moves. A fleet takes it by an ordinary rebuild, and a node that states no budget takes the ratio where
  its host can spare it (Decision 2, amended the same day).
* Builds on: [0052](0052-palw-qwen36-hybrid-class.md) (the mapped container: a 33 GiB file the
  runtime maps rather than reads, and the note that residency was left to the page cache),
  [0067](0067-a-registered-class-is-served-by-any-node-that-holds-its-artifact.md) (a node holds
  what it chooses; `--palw-class-cache-bytes` bounds which artifacts load — this ADR bounds what
  of one artifact is in memory), [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (every chain term at 2M is constant or logarithmic; the model's cost is the host's), and
  [0110](0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md)
  (a figure is a measurement with a host and a time, never a guess).
* Supersedes: the residency paragraph of ADR-0052 §"what remains" and the `MADV_WILLNEED` expert
  cache it described (§7). Amends nothing in consensus.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L139: 3. Decisions / L343: 10.1 Where each Decision lives / L407: 10.5 The fleet's hosts, and the default amended (2026-09-11) / L428: Mission alignment amendment — 2026-10-07

<a id="source-0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height"></a>

## 0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md

[原文](../adr/0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) / [新しい主担当](0023-model-reward-routing-and-owner-leg.md)

原文 L14–L28

~~~~~~~~text
* Status: PROPOSED 2026-09-11 at the operator's request ("追加で現在の売買の burn 5% 開発者への手数料 1% から
  burn 5% 開発者への手数料 5% にあげて　つまり売って買うと 20% は fee として持ってかれるようにして").
  **IMPLEMENTED the same day**, and **scheduled on testnet-11 at DAA 3,500** by the operator the same
  day (asked with the chain at ≈3,399: "DAA 3,500 で有効化"): `PALW_RC_MODEL_LEG_V2_FENCE_DAA`. Every
  other preset keeps `palw_model_leg_v2 = None`. testnet-11's printed fingerprint moves `ecbdbc22…` →
  `02c7282b…`; the identity does not, so the fleet rolls one host at a time and builds with and without
  the fence stay peers until 3,500 (`the_owner_leg_flag_day_keeps_every_current_node_until_3500`).
  **3,500 is a flag day: every node must run a build carrying the fence before it.**
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decision 4 (the split of
  every BILI leg: 5 % burned, 1 % to the class's registrant), [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)
  Decision 8 (that leg is the line OWNER's, and an adopted contributor takes `contributor_permille_of_leg`
  of it — of the five percent past the fence), [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
  (the window's `quoteBuy`/`quoteSell` and `constants()`), and [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  §4's worked table (a second table beside it, §2).
* Supersedes nothing. Leaves ADR-0091 (the reward's 5 % buys the pair, no leg) untouched.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L37: 1. Decisions

<a id="source-0115-a-pending-transaction-is-announced-until-it-lands"></a>

## 0115-a-pending-transaction-is-announced-until-it-lands.md

[原文](../adr/0115-a-pending-transaction-is-announced-until-it-lands.md) / [新しい主担当](0004-mass-carriage-and-transaction-relay.md)

原文 L14–L24

~~~~~~~~text
* Status: PROPOSED 2026-09-11, from the operator's report ("また購入してもトランザクションやチャートに反映されてない
  原因は" / "何回購入しても balance が減らない") and their standing instruction to put liveness first.
  **IMPLEMENTED the same day, consensus-inert**: node policy only — no validity rule, no `Params`
  field, no fingerprint moves. A node takes it by a rebuild and restart; a node without it still
  validates every block the others make.
* Builds on: [0020](0020-selected-parent-evm-lane.md) (the EVM lane: a payload is carried by a block and
  executed when the chain accepts it), [0072](0072-the-ticket-is-the-execution.md) (a producer's template
  is fixed for the whole draw), [0109](0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md)
  (the same failure for deposit claims, fixed there by making every producer claim unasked).
* Amends: kaspa-pq EVM Lane v0.4 §14.2 (EVM tx gossip) and §16 (mempool retention).
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L51: 2. Decisions

<a id="source-0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width"></a>

## 0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md

[原文](../adr/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md) / [新しい主担当](0013-tir-arithmetic-and-determinism.md)

原文 L15–L26

~~~~~~~~text
* Status: PROPOSED and IMPLEMENTED 2026-09-11 on `feat/adr-0103-held-context`, at the operator's
  instruction. The same day ADR-0103 §10.7 recorded a decision to keep the bound; the operator then
  reversed it and asked for the change ("A16 の履歴上限 2^18 … の実装を行う方針に変更する").
  Consensus-inert on every shipped network: the bound a class reads is fixed by the held map
  registered in its class id, and the gate admits a held class only while `Params::palw_held_context`
  is armed. No preset's fingerprint moves.
* Builds on: [0040](0040-palw-base-0-integer-arithmetic.md) (Decision E: exact integer accumulation within
  a proven bound, which the A16 tier inherits), [0082](0082-the-close-is-flat-in-the-context.md)
  (the fused attention site and its tile route), [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the held regime, whose widest context is 2M), [0110](0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md)
  (the context vectors that found the wall).
* Reverses: ADR-0103 §10.7's "Decided 2026-09-11: the bound stays."
~~~~~~~~

判断・改定等を含む見出し（抽出）: L66: 3. Decisions / L172: Mission alignment amendment — 2026-10-07

<a id="source-0117-a-draw-is-one-forward"></a>

## 0117-a-draw-is-one-forward.md

[原文](../adr/0117-a-draw-is-one-forward.md) / [新しい主担当](0026-canonical-work-and-answer-accounting.md)

原文 L15–L32

~~~~~~~~text
* Status: PROPOSED and IMPLEMENTED 2026-09-11 on `feat/adr-0103-held-context`, at the operator's
  instruction. The same day ADR-0112 §8 had recorded the one-forward ticket as not taken; the
  operator reversed that and asked for the change ("1 回の forward で抽選する … 実装を行う方針に変更する").
  Decision 2 (the engine) is consensus-neutral and in force wherever this build runs. Decision 1
  (the job) is behind `Params::palw_prefill_draw`, `None` on every shipped preset, so no fingerprint
  moves until a network arms it.
* **Scheduled on testnet-11 at DAA 4,000** (operator, 2026-09-11), on the pre-mainnet audit's flag day
  (`PALW_RC_AUDIT_FENCE_DAA`), so one rollout carries both; t11's fingerprint moves `09efd285…` →
  `4300409b…`. The shared height has one consequence the fork-id gate cannot see: a build carrying
  the audit fence without this one advertises the same fork id and parts silently at 4,000 (the
  constant's doc). Held context (ADR-0103) was asked for the same height and given its own flag day
  instead — its prerequisites on testnet-11 (signature contexts, Merkle prompt ids, the shard court)
  are not armable yet.
* Builds on: [0072](0072-the-ticket-is-the-execution.md) (the ticket is a function of the
  execution), [0084](0084-the-ids-ride-the-capture-stays-home.md) (Decision 7: the seat's verdict by
  execution), [0112](0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md)
  (the weights are read within a budget, and a draw's cost is what it reads).
* Reverses: ADR-0112 §8's "Decided 2026-09-11: not taken."
~~~~~~~~

判断・改定等を含む見出し（抽出）: L80: 3. Decisions / L256: Mission alignment amendment — 2026-10-07

<a id="source-0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form"></a>

## 0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md

[原文](../adr/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md) / [新しい主担当](0018-long-context-measurement-and-admission.md)

原文 L15–L35

~~~~~~~~text
* Status: PROPOSED and IMPLEMENTED 2026-09-12 on `feat/adr-0103-held-context`, at the operator's
  instruction ("held方式の実装完了をgoalとする"). The operator decided the same day that the
  regime takes a flag day of its own on testnet-11, at a height they will choose, and not the
  audit's 4,000 — and, asked again on 2026-09-12, chose **DAA 7,000**, the deep-audit fence's
  height, so one release carries both; **moved to DAA 6,000 on 2026-09-17** (the operator: the wait was
  too long; the tip was 5,798, and the deployed 7,000 release never reaches its height — it is refused
  from 6,000). testnet-11 therefore arms the regime at 6,000
  (`PALW_RC_HELD_FENCE_DAA = Some(6_000)`; its fingerprint on this branch alone was `80524c3b…`,
  merged with the 4,000 release that carries `palw_prefill_draw` it is `c2e5ffbe…`, and the release
  that also carries the deep-audit fence is re-pinned when it is assembled). Every other preset leaves the regime dormant.
* Builds on: [0081](0081-long-context-the-input-is-a-state-chain.md) (Decision 3: the prompt-ids
  form), [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the held regime), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) (the
  held DA court's leaf demand), [0116](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md)
  (the history bound is the class's — the same move this ADR makes for the prompt form).
* Amends: ADR-0103's assembly rules ("V4 and the tiled prompt ids at the fence") to "from genesis";
  ADR-0081 Decision 3's "the form is the network's" to "the form is the class's, and the network's
  for every class but a held one".
* Amended by: [0119](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md)
  (the same day), which lifts §4's limitation at the chain — a held class is walked at the regime's
  `2^40` ladder — and gives §4's PanelDa jobs their executor path.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L86: 3. Decisions / L279: Mission alignment amendment — 2026-10-07

<a id="source-0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are"></a>

## 0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md

[原文](../adr/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md) / [新しい主担当](0018-long-context-measurement-and-admission.md)

原文 L15–L30

~~~~~~~~text
* Status: PROPOSED and, for its consensus half, IMPLEMENTED 2026-09-12 on
  `feat/adr-0103-held-context`, at the operator's instruction ("ADR-0119 に進める"), after ADR-0118
  §4 measured that testnet-11's frozen ladder admits a held dense class to about 512 positions.
  Rides testnet-11's held flag day, DAA 6,000 (ADR-0118 Decision 6). **Inert wherever the held
  regime is not in force:** the new state collection is empty until a held class registers, which
  nothing below the fence can do; the transaction door keeps its validity below the fence; every
  class that is not held is walked, priced and bounded exactly as before.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the regime, and Decision 1's "a ladder that costs no rounds is minted at the top of the carrier's
  budget"), [0118](0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md)
  (the regime at a height; the class owns its prompt form — the move this ADR makes for the ladder),
  [0116](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md)
  (the history bound is the class's).
* Amends: ADR-0103 Decision 1's `2^48` example ladder (a held network's ladder is at most `2^40`);
  ADR-0082's "the opening cap is the structural one" for a fused site's rows (under the held
  regime it is the claim's ladder).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L84: 3. Decisions / L236: Mission alignment amendment — 2026-10-07

<a id="source-0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height"></a>

## 0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md

[原文](../adr/0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md) / [新しい主担当](0022-liquidity-seeding-and-virtual-reserves.md)

原文 L14–L25

~~~~~~~~text
* Status: PROPOSED 2026-09-12 at the operator's request ("misaka position のモデル追加の際に流動性として
  ロックして引き出せないようにする misaka の数を 1M として　今の 100000 枚から引き上げて"). **IMPLEMENTED
  the same day** on `feat/adr-0120-seed-min-1m`, and **scheduled on testnet-11 at DAA 6,900** by the
  operator the same day, to ship with the 7,000 release at a height of its own (unchanged on 2026-09-17,
  when the operator moved the 7,000 heights to 6,000: 6,900 now follows them):
  `PALW_RC_MODEL_SEED_V2_FENCE_DAA`. Every other preset keeps `palw_model_seed_v2 = None`.
  **6,900 is a flag day: every node must run a build carrying the fence before it.**
* Amends: [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decision 2 (the least seed, 100,000 BILI) and [0094](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md)
  (the floor a pledge collects toward).
* Builds on: [0114](0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) (a market rule
  that changes what the fold writes arrives by activation, read through one resolved fence).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L41: 2. Decision

<a id="source-0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders"></a>

## 0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md

[原文](../adr/0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md) / [新しい主担当](0017-model-streaming-and-memory-bounds.md)

原文 L15–L34

~~~~~~~~text
* Status: PROPOSED and IMPLEMENTED 2026-09-12 on `feat/adr-0103-held-context`, at the operator's
  instruction ("node 側の held 経路（ADR-0120、数日規模）に進む" — the number moved to 0121 when the
  model-seed session claimed 0120 the same day, §8). The node half of
  [ADR-0119](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md),
  whose §7 mapped it. **Consensus-inert:** no consensus object, rule, parameter or fingerprint
  moves; every object a node now builds by streaming is byte for byte the object the whole-capture
  path built, and every class that is not held is served, checked and named exactly as before. Rides
  testnet-11's held build (DAA 6,000) because that is the build in which a held class first exists.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the regime; Decision 2's intervals of `P` positions), [0119](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md)
  (a held class's ladder is the regime's `2^40`), [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  (the V4 opening is the fold's digests; a seat names the leaf from a served block),
  [0085](0085-the-close-is-assembled-from-what-was-served.md) (the close annex and the close from
  served intervals), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)
  (a leaf's evidence), [0082](0082-the-close-is-flat-in-the-context.md) Decision 7 (the fold).
* Amends: ADR-0086 Decision 6's block index, for a held class only (§3 D3), and its block answer,
  which now serves a block straddling a range's end as its leaves inside the range, named by the
  range's root walk (§3 D5); ADR-0082 Decision 7's "the retain level is the ruleset's ladder's", for
  a held class only (§3 D3). Closes ADR-0119 §7's
  first three items; §7 below keeps the rest.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L80: 3. Decisions / L289: Mission alignment amendment — 2026-10-07

<a id="source-0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id"></a>

## 0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md

[原文](../adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md) / [新しい主担当](0029-remote-client-mining-and-host-sandbox.md)

原文 L17–L29

~~~~~~~~text
* Status: PROPOSED 2026-09-12 on `feat/adr-0122-operator-ux`, at the operator's request: turn PALW
  mining from a set of components into a product ("マイニング 検証を立てる やモデル追加 モデルポジション
  などの使いやすさをこのように引き上げて"). The request came with a proposed shape (purpose-level
  verbs, a per-work state machine, human errors, one-command start, structured logs, a dashboard,
  a wizard) and a priority order, ① to ⑧, which §9 follows. **Consensus-inert:** no consensus object,
  rule, parameter, fence or fingerprint moves. Everything here is the CLI, three additive RPC reads,
  and log lines. Existing commands, flags and log lines keep working as they are today.
* Builds on: the `misaka palw claim` reader (its `state`/`meaning`/`next` rows are §3's chain half),
  `getPalwProducerFacts` (ADR-0042 D6, and its `not_ready_reason`), the retention janitor
  (`kaspad/src/palw_retention.rs`), the fleet's roll procedure (readiness gates and the stop grace,
  §6), and the free-prompt lane's documented onboarding (`docs/history/testnet11-free-prompt-mining.md`).
* Amends nothing. Supersedes the stale `misaka setup` web flow for mining (testnet-10, `misaminer`)
  as the documented path. That flow stays in the tree for testnet-10's DNS validator hosts.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L390: 6.4 What status and rewards read (existing reads first) / L983: Mission alignment amendment — 2026-10-07

<a id="source-0123-the-epoch-progressively-releases-unused-class-budget"></a>

## 0123-the-epoch-progressively-releases-unused-class-budget.md

[原文](../adr/0123-the-epoch-progressively-releases-unused-class-budget.md) / [新しい主担当](0020-class-share-and-allocation-evolution.md)

原文 L12–L16

~~~~~~~~text
* Status: PROPOSED and IMPLEMENTED 2026-09-12 on `feat/adr-0123-epoch-budget-release`, at the
  operator's request after the testnet-11 budget stall. The rule is dormant on every shipped
  preset until `Params::palw_epoch_budget_release` is activated.*
* Builds on: [0045](0045-palw-class-economy-on-chain.md)
  (per-class epoch budgets), and the existing `palw_epoch_boundary_budget` fence.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L25: 2. Decision

<a id="source-0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies"></a>

## 0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md

[原文](../adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md) / [新しい主担当](0037-fee-escrow-and-role-consent.md)

原文 L17–L48

~~~~~~~~text
* Status: PROPOSED and IMPLEMENTED 2026-09-17 on `feat/adr-0124-panel-reward-and-compute-weight`
  (from `main` at `6fdf6ba7`), at the operator's request ("現在 PANEL に報酬がないのと小さいモデルと大きい
  モデルで同じ 1 claim 1 ブロックだと圧倒的に小さいモデルが有利な点を改善する … ADR を作成し実装を行なって";
  the design the operator brought is quoted in §1). Behind two bare fences,
  `Params::palw_panel_economy` (Decisions 1–5) and `Params::palw_work_priced_reward` (Decision 6).
  **testnet-11 schedules both at DAA 6,001** (the operator's flag day of 2026-09-17, with ADR-0125,
  ADR-0126 and ADR-0128; §8); `None` on every other shipped preset. **A mainnet card states both from
  genesis** (Decision 7).
* Builds on: [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 10 (the worker reward is a
  carve of the subsidy, escrowed at the accepting block and named at `Final`, never minted before),
  [0045](0045-palw-class-economy-on-chain.md) Decision 1 (`pwu` has one legal value) and Decision 3
  (the share table is chain state), [0061](0061-zero-seat-genesis-and-right-sized-collateral.md)
  Decision 2 (10,000 BILI per genesis seat), [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md)
  Decision 4 (`Unavailable` abstains), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md)
  Decision 7 (an uncertified class bears no weight — the classes that set the unit are the ones that
  bear weight), [0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md)
  (the seed is share × pwu per inference), [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
  (the buyback is a slice of the reward), [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
  Decision 2 (a seat that found a lie files nothing), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)
  Decision 3 (a per-claim table that enters the root only once written — the shape the duty row
  reuses), [0114](0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) §3 (why a
  height and not an edit).
* Amends: [0038](0038-palw-is-the-consensus-work.md) Decision D's inter-class clause — "`pwu`
  magnitude must never be read as a cross-class price" — **for the payout and the payout alone**
  (Decision 6, §2.3): fork choice, the budget and the retarget still read no pwu across a class
  boundary; [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 10 (the carve named at `Final`
  is now split and priced before it is named); [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
  Decision 1 (the slice is five percent of the escrow AS PRICED, so a lighter class's pair buys
  proportionally); the 2026-09-11 mainnet audit's C-02 (the stake-weighted seat draw is retired
  past the panel-economy fence, Decision 5) and BC-SYBIL (a dissenting seat's stake is its
  reservation, three times the claim's, Decision 3).
* Supersedes nothing. Leaves ADR-0098 Decision 2 in force and names its cost (§6, SA-4).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L136: 3. Decisions / L284: 6. Security — the four principles, checked, and the amendments stated before the build / L442: Mission alignment amendment — 2026-10-07

<a id="source-0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time"></a>

## 0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md

[原文](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md) / [新しい主担当](0007-evm-exec-lane-and-anchors.md)

原文 L17–L33

~~~~~~~~text
* Status: **IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement` (from `main` at
  `6fdf6ba7`, after ADR-0124), including the stage table (§7.2), equivocation evidence and relay
  de-duplication (§7.3), the RPC read and the operator's status line (§7.4), and the devnet drill
  (§7.1). **testnet-11 schedules it at DAA 7,101 at one permit per one-second round**. `6001` is the
  bundle's retained logical label in historical audit and test material, not a second activation
  height; `None` on every other shipped preset.
* Operator's request: "testnet では BPS 1 から実装して最終目標を BPS 10 とする", with the design
  quoted in §1.
* Builds on: [0060](0060-the-liveness-doctrine.md) / [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
  (a lane with its own algorithm id, a constant target that never touches `bits`, zero subsidy),
  [0072](0072-the-ticket-is-the-execution.md) (an attempt's execution commitment is a value nobody
  re-rolls for free), [0075](0075-certification-is-a-consensus-object.md) (a certified family is chain
  state), [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md) (what a second lane does to a
  bonded block at `ghostdag_k = 1`), [0107](0107-a-share-grows-on-work-that-reached-final.md) (`Final`
  is what is counted), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md).
* Amends nothing: no DAG parameter, no PALW window and no cadence moves (§8 says why an earlier draft
  thought they had to). Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L83: 3. Decisions / L230: 5. Security amendments / L363: Mission alignment amendment — 2026-10-07 / L371: Semantic mergeset amendment - 2026-10-07

<a id="source-0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays"></a>

## 0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md

[原文](../adr/0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms"></a>

## 0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md

[原文](../adr/0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md) / [新しい主担当](0006-fork-choice-participation-and-settlement.md)

原文 L15–L26

~~~~~~~~text
* Status: **ACCEPTED and IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement`.
  No consensus rule changes and no fence: this ADR names things, states a property the code already has,
  and makes CI keep it.
* Operator's direction, in the operator's words: "PALW は DNS finality や BFT バリデーター依存ではないことを
  明記して"; "PALW Final ≠ DNS Final / PALW Settlement Anchor ≠ DNS Anchor / PALW Panel ≠ DNS Validator を
  コード・型・ドキュメント上でも完全に分けるべき — Anchor Finality という言葉も避けて PALW Settlement
  Finality / PALW Settlement Anchor / PALW Future Anchor くらいに統一"; "PALW settlement path から
  dns_confirmation、validator、beacon が一切参照されないことを CI で保証する".
* Builds on: [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 9 (the one fork-choice authority),
  [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) D2. The double-spend defence
  built on these terms is [0129](0129-a-double-spend-needs-the-anchors-not-the-blocks.md); the DNS
  overlay that runs beside PALW is [0128](0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L47: 2. Decisions / L95: Mission alignment amendment — 2026-10-07

<a id="source-0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate"></a>

## 0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md

[原文](../adr/0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L5–L5

~~~~~~~~text
**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
~~~~~~~~

<a id="source-0129-a-double-spend-needs-the-anchors-not-the-blocks"></a>

## 0129-a-double-spend-needs-the-anchors-not-the-blocks.md

[原文](../adr/0129-a-double-spend-needs-the-anchors-not-the-blocks.md) / [新しい主担当](0006-fork-choice-participation-and-settlement.md)

原文 L14–L28

~~~~~~~~text
* Status: **ACCEPTED and IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement`.
  No consensus rule and no fence of its own: it states which existing rules defend a payment, pins each
  with a test, and adds the read that counts confirmations in anchors. testnet-11 runs it from DAA 6,001
  with the execution lane at one block a second (ADR-0125 §7.5) and the seat exposure (ADR-0124) armed at
  the same height.
* Operator's direction, in the operator's words: "『モデルを交互にする』だけを二重支払い防止の主役にしない
  方がいい"; "Anchor Finality / Panel quorum certificate / Execution block の Finality weight = 0 / 同一 UTXO の
  競合を DAG 順序で一意化 / Producer・Panel equivocation slash / model・operator share cap / 高額決済の追加
  confirmation"; "10 BPS は UX/throughput、120 秒 PALW Anchor は security/finality と役割を完全に分ける";
  "これを ADR129 として記述して実装して — DAA 7001 から BPS1 になることを想定して進めている".
* Builds on: [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (execution blocks), [0127](0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md)
  (the settlement anchor and its names), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (what a seat holds and loses), [0072](0072-the-ticket-is-the-execution.md) (an attempt is its header
  position), [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 9 (the one fork-choice authority).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L48: 2. Decisions / L122: 4. Security amendments

<a id="source-0130-bps1-is-hardened-before-it-is-widened"></a>

## 0130-bps1-is-hardened-before-it-is-widened.md

[原文](../adr/0130-bps1-is-hardened-before-it-is-widened.md) / [新しい主担当](0011-release-activation-and-flag-days.md)

原文 L17–L30

~~~~~~~~text
* Status: **ACCEPTED 2026-09-17; Decisions 2–6 IMPLEMENTED the same day, Decision 1 built dormant, Decisions 7–8 not built** — on `feat/palw-exec-lane-and-validator-retirement` (§7).
  Decisions 2–5 ride testnet-11's DAA 6,001 flag day with ADR-0124/0125/0126/0128. Decision 1 is built
  dormant and computed in shadow (Decision 8); the operator decided (2026-09-17) that λ is off at 6,001 and
  gets a height only once the shadow shows the capacity and a shorter liability lifetime exists (§3).
* **ADR-0144 alignment (2026-09-21).** §5 deferred widenings stay deferred under 0144 item 0. See the end section.
* Operator's direction, in the operator's words: "BPSは当分1で固定するなら、今は高速化ではなく『BPS1でも壊れ
  ない経済性・選出・settlement』を固める段階"; "一番大きい変更を1つだけ選ぶなら、まず
  `panel_exposure = max(3×claim.reserved, λ×seat_reward)`"; "その次が BPS1でのcross-round operator連続禁止、
  その次が claim由来seedから『snapshot後のfuture DAG anchor由来seed』への変更"; "代替operatorがいない →
  round miss"; "安全性のためのルールを、参加者不足だからこっそり解除するのは危険".
* Builds on: [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (the panel's pay and a seat's exposure), [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (the lane and its schedule), [0129](0129-a-double-spend-needs-the-anchors-not-the-blocks.md) (settlement
  counts anchors), [0072](0072-the-ticket-is-the-execution.md) (a block's draw is its header position).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L68: 2. Decisions / L166: 4. Security amendments and residuals / L274: Mission alignment amendment — 2026-10-07

<a id="source-0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves"></a>

## 0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md

[原文](../adr/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) / [新しい主担当](0026-canonical-work-and-answer-accounting.md)

原文 L17–L29

~~~~~~~~text
* Status: **PROPOSED 2026-09-17; Decisions 1 and 2 IMPLEMENTED the same day, in shadow** on
  `feat/palw-exec-lane-and-validator-retirement` (§7). No consensus rule, parameter or fingerprint
  moves: everything here is node-local measurement until Decision 3 gets a height.
* **ADR-0144 alignment (2026-09-21).** Decisions 3–6 do not arm a scalar CCU until ADR-0146. See the end section.
* Operator's direction, in the operator's words: "次に見るべきは claim数ではなく `BILI / canonical compute`";
  "PWU = fork choice / consensus work、CCU = model間の経済価格 を分離"; "M4で何秒だったかを直接consensus値に
  しない — 時間は校正用データに留めます"; "DAA補正を二重に掛けないこと"; "Producer CCUとPanel CCUを分離";
  "45% domain capは報酬計算から切り離す"; "新モデル追加時に、shadow期間を必須にする"; "ここからモデル間の歪みを
  なくす方針で進める".
* Builds on: [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  Decision 6 (the work price this replaces), [0039](0039-palw-only-block-production.md) and
  [0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) (per-class targets), [0117](0117-a-draw-is-one-forward.md) (a draw is one
  forward), [0130](0130-bps1-is-hardened-before-it-is-widened.md) (the shadow-first discipline).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L74: 2. Decisions / L162: 4. Status / L186: Mission alignment amendment — 2026-10-07

<a id="source-0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price"></a>

## 0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md

[原文](../adr/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) / [新しい主担当](0026-canonical-work-and-answer-accounting.md)

原文 L17–L28

~~~~~~~~text
* Status: **PROPOSED 2026-09-17; the end-to-end shadow (§6) built the same day** on
  `feat/palw-exec-lane-and-validator-retirement`. No consensus rule, parameter or fingerprint moves.
* Operator's direction, in the operator's words: "価格式だけ公平ではなく … 実際に支払われた BILI まで含めた end-to-end
  の actual BILI / attempted Economic CCU をモデル間で可能な限り一致させること"; "Final 率差の原因が protocol 上の
  不公平なのか、単なる runtime / artifact availability 問題なのかを分離"; "compute cost と data movement /
  availability cost は分離"; "モデル名ごとの手書き固定倍率は禁止"; "5 年後にモデルが 10 種類になっても破綻しにくい案".
* Builds on: [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) (economic
  compute, the three bases, op 185), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (the panel split and the work price), [0130](0130-bps1-is-hardened-before-it-is-widened.md) (shadow first),
  [0072](0072-the-attempt-is-a-function-of-the-execution.md) (one execution, two lotteries),
  [0071](0071-hash-coupling-and-the-remainder.md) SA-3 (`palw_capability_bound`), [0117](0117-a-draw-is-one-forward.md),
  [0098](0098-a-panels-coverage-of-one-claim-is-a-number.md), [0107](0107-share-growth-is-counted-at-final.md).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L103: 2. The end-to-end numbers, per accepted claim (expectation; p_net at the run's 0.206) / L366: Mission alignment amendment — 2026-10-07

<a id="source-0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself"></a>

## 0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md

[原文](../adr/0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md) / [新しい主担当](0034-verification-spans-and-service-clock.md)

原文 L17–L31

~~~~~~~~text
* Status: **PROPOSED 2026-09-17; the profile, the capacity arithmetic, the simulation and the observability
  built in shadow the same day** on `feat/palw-exec-lane-and-validator-retirement`. No consensus rule,
  parameter or fingerprint moves; nothing activates on testnet-11.
* Operator's direction, in the operator's words: "120秒を「全modelがverificationを完了しなければならない
  deadline」とは扱わない"; "Execution cadence / PALW anchor cadence / class verification deadline を別々の時間軸
  として設計"; "大型modelのliveness不足がPALW全体やExecution Lane全体を停止させないこと"; "verification window を
  producer 自身が選択できないこと"; "Compute Credit は Final 後にのみ発生"; "artifact 転送が遅いことを Economic CCU
  へ単純に加算しないこと"; "BPS1 と PALW Anchor 120 秒は分析なしに変更しない"; "beacon を復活させない".
* Builds on: [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md)
  (the end-to-end read), [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md)
  (economic compute), [0130](0130-bps1-is-hardened-before-it-is-widened.md) (spans, the shadow-first discipline),
  [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (credits from `Final`s), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (seat exposure), [0112](0112-the-weight-residency-budget.md) (residency), [0098](0098-a-panels-coverage-of-one-claim-is-a-number.md)
  (what sampling catches), [0071](0071-hash-coupling-and-the-remainder.md) SA-3 (capability by production).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L98: 3. Decisions / L460: Mission alignment amendment — 2026-10-07

<a id="source-0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes"></a>

## 0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md

[原文](../adr/0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L12–L24

~~~~~~~~text
* Status: **ACCEPTED and IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement`. testnet-11
  schedules the retirement at DAA 6,201 — its own height, not the 6,001 flag day's (§4). The stateful machinery
  is deleted in the same change (§3), on the evidence of §2.
* Operator's direction, in the operator's words: "DNS/VLT の committee beacon の使用されてない経路 コードの削除も追加で
  完了して — これはバリデータに依存してる — PALW はバリデーターを巻き込まないように進めて — 現在の PoW を LLM に置き換える
  方針からずれているため使用しない"; and, on the order of work: "DNS/VLT/beacon 削除も今回と混ぜない … 同じ activation
  fence には入れない方がデバッグしやすい".
* Builds on: [0024](0024-verified-llm-token-weighted-bft.md) (the overlay this retires), [0126](0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md)
  (what ADR-0126 already removed and what it kept "because testnet-10 and testnet-11 run its shadow"),
  [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) /
  [0133](0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md)
  (the fences this one is kept apart from), the memo *Codex's beacon/VLT cleanup is a silent fork* (why a deletion
  needs a height and evidence).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L60: 3. Decisions / L96: 4. Security amendments

<a id="source-0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle"></a>

## 0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md

[原文](../adr/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md) / [新しい主担当](0015-permissionless-model-registration.md)

原文 L15–L31

~~~~~~~~text
* Status: **PROPOSED 2026-09-17; the rule set built in shadow the same day, and Protocol Upgrade A built behind a
  dormant fence the same evening** (`palw_model_registry_v1.rs`, `palw_state_v2.rs`; §7). No shipped preset arms
  it: no parameter or fingerprint moves until the operator schedules `palw_model_registry` at a height.
* Operator's direction, in the operator's words: "誰でもモデルを chain へ登録でき、検証時間・必要 Panel 数・claim 上限・
  報酬係数までプロトコルが自動計算し、条件を満たしたら自動 activate"; "モデルをコードではなくデータとして追加する";
  "実測値を consensus parameter にしない"; "Panel 側も permissionless — 自己申告だけではダメ"; "モデル登録から
  activation まで全部 state machine"; "share も最終的には手動設定をなくしたい"; "既存 Canonical ML VM で表現可能 →
  permissionless、新しい opcode が必要 → protocol upgrade".
* Supersedes, in ADR-0133: Decision 2's *measured* p99 as an input to the profile (measurements become telemetry),
  §9a's "profile freeze by the operator" and "tiny activation → ramp by the operator" (the lifecycle does both).
  Keeps everything else of ADR-0133 (three clocks, Little's law, class-local fail-closed, the artifact options).
* Builds on: [0133](0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md),
  [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md),
  [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) (economic compute from the
  graph), [0049](0049-palw-adjudication-contract.md) (the canonical IR the VM boundary is drawn on), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  and [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) (the on-chain class registration and carriage this extends),
  [0056](0056-palw-permissionless-class-admission-and-share-economy.md) (class admission is permissionless already; this ADR removes the share it kept), [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) (who may judge a class).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L64: 2. Decisions / L154: 5. Security amendments / L430: Mission alignment amendment — 2026-10-07

<a id="source-0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it"></a>

## 0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md

[原文](../adr/0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md) / [新しい主担当](0017-model-streaming-and-memory-bounds.md)

原文 L12–L13

~~~~~~~~text
Status: PROPOSED 2026-09-17 on `feat/palw-exec-lane-and-validator-retirement`, with its first two decisions
implemented the same day. Node-side only: no consensus rule changes, no fingerprint moves.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L43: 2. Decisions

<a id="source-0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input"></a>

## 0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md

[原文](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md) / [新しい主担当](0020-class-share-and-allocation-evolution.md)

原文 L17–L24

~~~~~~~~text
**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`, design with a
reproducible simulation (`scripts/sim/palw-share-sim.py`); **the shadow (§17 step 1) built the same
day** — `palw_work_target_v1.rs`, the state's `work_target_shadow` and `final_work` carried outside
the root, op 186's work-target block and per-class `CCU / W`, ticket, room and finalized-work
shares (§22); nothing armed, no fingerprint moves. The interim commit `804af11d` (a class the
registry seats is priced by `attempt_target_seed_v1`) stays as a shadow of the shipped rule and is
**not** the final design — §6 says why. **Arming ADR-0135 D5's shares on testnet-11 is stopped**
(§3.5, §17): the registry's rows, proofs and gate may arm; the share → target rule may not.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L718: Mission alignment amendment — 2026-10-07

<a id="source-0138-the-daa-score-is-the-anchors-clock"></a>

## 0138-the-daa-score-is-the-anchors-clock.md

[原文](../adr/0138-the-daa-score-is-the-anchors-clock.md) / [新しい主担当](0008-daa-difficulty-and-consensus-clock.md)

原文 L12–L15

~~~~~~~~text
**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`; **built and fenced**
(`Params::palw_anchor_clock`, `Some(6,001)` on testnet-11 — the single lottery's own day — `None`
elsewhere). Found by the operator's DAA-clock audit (`docs/palw-daa-clock-audit-2026-09-18.md`),
which held the rollout of main and testnet-11 until this was closed.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L31: 2. Decision

<a id="source-0139-the-execution-lanes-gas-is-one-budget-a-round"></a>

## 0139-the-execution-lanes-gas-is-one-budget-a-round.md

[原文](../adr/0139-the-execution-lanes-gas-is-one-budget-a-round.md) / [新しい主担当](0007-evm-exec-lane-and-anchors.md)

原文 L12–L16

~~~~~~~~text
**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`; **built**. Rides
`palw_execution_lane` (testnet-11 DAA 6,001): below the lane the cap is what a chain block always
had; past it the cap is a function of the round blocks the chain block merges. Decides the open
item O13 of `docs/misaka-evm-design-v0.4.md` (§14.3: "per-second G_limit derivation + measured
propagation, frozen before activation").
~~~~~~~~

判断・改定等を含む見出し（抽出）: L28: 2. Decision

<a id="source-0140-the-heartbeat-is-the-emergency-generator"></a>

## 0140-the-heartbeat-is-the-emergency-generator.md

[原文](../adr/0140-the-heartbeat-is-the-emergency-generator.md) / [新しい主担当](0009-heartbeat-floor-and-liveness.md)

原文 L12–L15

~~~~~~~~text
**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`. **Changes no
consensus rule and arms no fence.** It settles what the goal is, keeps the hash puzzle, states the
non-interference invariant as five separate claims, and says which of them the code already
guarantees and which are only believed.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L214: 7. Decision

<a id="source-0141-can-an-inference-be-the-ticket-without-a-hash-lottery"></a>

## 0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md

[原文](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) / [新しい主担当](0028-attempt-ticket-and-seed-anchor.md)

原文 L15–L19

~~~~~~~~text
**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`. **This ADR decides
nothing about the lottery, changes no rule and builds nothing.** It states the question precisely,
records why it is the one place a hash is still worth reopening, lists what would have to be answered
before anything replaced it, and specifies the counters that would settle the first half of the
argument — to be built when the 7,101 rollout is behind us, not before.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L109: 4. Decision / L132: Mission alignment amendment — 2026-10-07

<a id="source-0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot"></a>

## 0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md

[原文](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) / [新しい主担当](0008-daa-difficulty-and-consensus-clock.md)

原文 L12–L18

~~~~~~~~text
**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`. **Consensus
change, built and drilled, NOT ARMED on any preset.** The four checks of §6a all pass; arming is the
operator's call and wants a reorg drill and a target-cadence drill first (§8).
**Amended 2026-09-24 (§9): the clock floor.** §6a check 4 and §6 property 6 did not hold once the
carried cursor was deleted — the step's timestamp was unconstrained, so the clock could run fast and a
future-stamped sibling could delay it. `Params::palw_clock_floor` (testnet-12 only) makes them true.
It replaces the heartbeat lane's admissibility rule. It held the 7,101 rollout.
~~~~~~~~

原文 L259–L262

~~~~~~~~text
**Status:** H3 and H5 are consensus, behind their own fence `Params::palw_clock_floor` — `Some(0)` on
testnet-12, `None` on every other preset (testnet-11, devnet and mainnet fingerprint byte-identically
to the build before the field: `consensus/core/tests/palw_clock_floor_is_t12_only.rs`). H1 and H2 are
node policy and move no fingerprint. Built on `hb/t12-heartbeat`, for the testnet-12 regenesis.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L257: 9. Amendment (2026-09-24): the clock floor — H1, H2, H3, H5 of the heartbeat audit

<a id="source-0143-an-artifact-root-has-one-owner-on-the-chain"></a>

## 0143-an-artifact-root-has-one-owner-on-the-chain.md

[原文](../adr/0143-an-artifact-root-has-one-owner-on-the-chain.md) / [新しい主担当](0016-artifact-ownership-and-model-lineage.md)

原文 L20–L23

~~~~~~~~text
**Status:** IMPLEMENTED 2026-09-18, **withdrawn from the 7,100 flag day and REWORKED** after the
adversarial audit of 2026-09-19 found two defects in the index this ADR installs. Both are fixed on
`fix/audit-2026-09-19`; `palw_artifact_root_ownership` stays `None` on every preset and arms on its
own day, after a drill crosses its own fence.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L12: 2026-10-09 改定 — 永久不変なrootと独立モデルの一意性 / L106: 3. Decision

<a id="source-0144-palw-pays-for-the-inference-you-were-going-to-run-anyway"></a>

## 0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md

[原文](../adr/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) / [新しい主担当](0026-canonical-work-and-answer-accounting.md)

原文 L15–L17

~~~~~~~~text
**Status:** CONSTITUTION, 2026-09-19. This ADR fixes what PALW is FOR. It changes no code and arms
no fence. Its purpose is to make the next four redesigns answerable: a proposal is in scope if it
serves the sentence below, and out of scope if it does not.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L291: Mission alignment amendment — 2026-10-07

<a id="source-0145-canonical-work-is-derived-and-admission-is-earned"></a>

## 0145-canonical-work-is-derived-and-admission-is-earned.md

[原文](../adr/0145-canonical-work-is-derived-and-admission-is-earned.md) / [新しい主担当](0026-canonical-work-and-answer-accounting.md)

原文 L15–L19

~~~~~~~~text
**Status:** DESIGN 2026-09-19; **§1–§5 implemented dormant** (ADR-0147/0148/0149, the economic
bundle, `None` on every preset). **§6 prefix-STATE: object family implemented 2026-09-21**
(`PalwFpPrefixStateV1`, `fp_derive_work_from_state_v1` derives `KvReused`). The V3 commitment
wire is unchanged — a field addition would be a new object family, and live claims persist. A
V3 commitment that names no state is genesis. No new fence.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L300: Mission alignment amendment — 2026-10-07

<a id="source-0146-a-coefficient-is-justified-by-the-arbitrage-it-permits"></a>

## 0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md

[原文](../adr/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) / [新しい主担当](0020-class-share-and-allocation-evolution.md)

原文 L12–L17

~~~~~~~~text
**Status:** SEARCH IMPLEMENTED 2026-09-21. No coefficient table, no fence, no `Params` field.
`palw_arbitrage_search_v1` is the committed program Rule R7 asked for (`cargo test -p
kaspa-consensus-core palw_arbitrage_search`). The bound it reports for the derivation is
1.000000× across the four shipped classes and across the admissible declaration space of one
class, so there is no table to write and no scalar to arm. §6 (resident vs paged) stays an open
cost question, not a price lever.
~~~~~~~~

<a id="source-0147-independence-is-drawn-not-declared"></a>

## 0147-independence-is-drawn-not-declared.md

[原文](../adr/0147-independence-is-drawn-not-declared.md) / [新しい主担当](0031-panel-independence-and-unavailability.md)

原文 L15–L18

~~~~~~~~text
**Status:** IMPLEMENTED behind `Params::palw_admission_independence`, which is one of the three
fences of the ADR-0145 economic bundle (`validate_palw_v2` arms them at one height or not at all).
Dormant on every preset. Supersedes the identity test that shipped under the same fence on
2026-09-19 (`palw_bond_is_independent_of_registrant_v1`), which is deleted.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L42: 2. Decision / L218: Mission alignment amendment — 2026-10-07

<a id="source-0148-the-free-prompt-lane-prices-compute"></a>

## 0148-the-free-prompt-lane-prices-compute.md

[原文](../adr/0148-the-free-prompt-lane-prices-compute.md) / [新しい主担当](0026-canonical-work-and-answer-accounting.md)

原文 L15–L19

~~~~~~~~text
**Status:** IMPLEMENTED behind the ADR-0145 economic bundle: `Params::palw_canonical_work` sets the
UNIT (a free-prompt claim accepted at or past it is priced in compute), `Params::palw_fp_derived_work`
the DERIVATION the price reads. The bundle rule arms them at one height, and a commitment that meets
the unit without the derivation is refused by name (`FreePromptComputeWithoutDerivation`). Dormant on
every preset.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L45: 2. Decision / L209: Mission alignment amendment — 2026-10-07

<a id="source-0149-an-attempts-pwu-is-the-derivation"></a>

## 0149-an-attempts-pwu-is-the-derivation.md

[原文](../adr/0149-an-attempts-pwu-is-the-derivation.md) / [新しい主担当](0026-canonical-work-and-answer-accounting.md)

原文 L15–L16

~~~~~~~~text
**Status:** IMPLEMENTED behind `Params::palw_canonical_work`, one of the three fences of the ADR-0145
economic bundle (`validate_palw_v2` arms them at one height or not at all). Dormant on every preset.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L55: 3. Decision / L155: Mission alignment amendment — 2026-10-07

<a id="source-0150-the-fingerprint-must-see-the-rule-not-only-the-height"></a>

## 0150-the-fingerprint-must-see-the-rule-not-only-the-height.md

[原文](../adr/0150-the-fingerprint-must-see-the-rule-not-only-the-height.md) / [新しい主担当](0001-network-identity-and-fingerprint.md)

原文 L12–L14

~~~~~~~~text
**Status:** ACCEPTED, 2026-09-20. Implemented behind no fence: the manifest is a description of the
build, and adding it re-pins every preset's fingerprint exactly once, on testnet, before mainnet
genesis freezes the format.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L80: 2. Decision / L124: 2.2 Which id each revision enters, and why it is not all of them

<a id="source-0151-liveness-is-structural-collateral-covers-fraud"></a>

## 0151-liveness-is-structural-collateral-covers-fraud.md

[原文](../adr/0151-liveness-is-structural-collateral-covers-fraud.md) / [新しい主担当](0009-heartbeat-floor-and-liveness.md)

原文 L17–L29

~~~~~~~~text
* Status: **D2, D3, D4 and D5 in force on testnet-12 from genesis; D1's genesis half landed, its
  runtime half open; D6 partially.** D4 and D5 were drafted as OPEN from a description of the defect
  and turned out to be already built — that correction is recorded in their sections rather than
  edited away.
  testnet-12 is the last testnet before mainnet, so the operator's decision (2026-09-22) was to start
  it on the separated design rather than ship the capital buffer and remove it later.
* Date: 2026-09-22
* Retires, on testnet-12: the `window_bind × dearest claim` genesis collateral requirement in
  `palw_genesis_v2` (`BondCannotSustainBindWindow`). It stays in force wherever the structural
  guarantee is not proved.
* Related: ADR-0061, ADR-0066 (the beat is outside `bits`), ADR-0071, ADR-0133 §7/§11.3,
  ADR-0137 (a share is a result), ADR-0138 §3b (the stand-in tick), ADR-0142 (the clock cursor),
  ADR-0144 §9 (the lock ledger), ADR-0145 (work is derived).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L63: 2. Decision / L366: Mission alignment amendment — 2026-10-07 / L376: Bond予算・総影響保存の改定 — 2026-10-10

<a id="source-0152-account-stake-staged-reserve-and-vested-rewards"></a>

## 0152-account-stake-staged-reserve-and-vested-rewards.md

[原文](../adr/0152-account-stake-staged-reserve-and-vested-rewards.md) / [新しい主担当](0035-bond-budget-and-shared-reservations.md)

原文 L12–L22

~~~~~~~~text
* Status: **Accepted for testnet-12** by the operator, 2026-09-24 (v3.1, with its 13 post-edits and
  the integration amendments IA-1…IA-15). **In force on testnet-12 from genesis** through
  `Params::palw_rcore_plus` (DAA 0, genesis only). Not armed on any other network. Amended after launch
  by the SW-8 seed correction of 2026-09-25 and by two flag days, [ADR-0154](0154-testnet-12-flag-day-daa-750.md)
  (DAA 750) and [ADR-0155](0155-testnet-12-flag-day-daa-1300.md) (DAA 1,300).
* Date: 2026-09-24
* Decided by: the operator. The binding decisions (v1, v2, v3, v3.1, the post-edits and the decisions
  after v3.1) are listed in the archive's
  [§1.5](../history/design/palw/0152/02-motivation-and-overview.md).
* Supersedes / amends: on testnet-12 only, ADR-0061's collateral model (with ADR-0151) and ADR-0124 D5
  with ADR-0130's one-ticket-per-operator panel lottery (by SW). ADR-0062 is amended by DA.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L35: Decision

<a id="source-0153-2m-verification-sharding-plan"></a>

## 0153-2m-verification-sharding-plan.md

[原文](../adr/0153-2m-verification-sharding-plan.md) / [新しい主担当](0030-sharded-verification-and-model-measurement.md)

原文 L5–L13

~~~~~~~~text
* Status: **Draft plan** (rcore/cap-s1, stage 7 of the user's capacity plan, 2026-09-27). **Decides
  nothing and arms nothing**: the 2M class stays capped — `c_2M = 1` held to `Final` (its row's static
  `max_inflight_claims`, which no ρ scales), on the C7 list (ADR-0152 D7, ADR-0160 D-13), uncredited:
  its escrow `E`, its seats' duty and lock and its `G` today's at every ρ (F1); only its weight
  reservation is `⌈w/ρ⌉` like every attempt's under F-W, the whole bond liable behind it (AG-2) — until
  the gates below pass and the user decides.
* Supersedes nothing. Related: ADR-0152 §4-quater (held attention, the 2M deadline redesign of
  2026-09-24), ADR-0160 (capacity; 2M is excluded from every ρ step's credit), ADR-0151 (liveness is
  structural), the kv-codec resource profile (2026-09-23).
~~~~~~~~

<a id="source-0154-testnet-12-flag-day-daa-750"></a>

## 0154-testnet-12-flag-day-daa-750.md

[原文](../adr/0154-testnet-12-flag-day-daa-750.md) / [新しい主担当](0011-release-activation-and-flag-days.md)

原文 L5–L13

~~~~~~~~text
* Status: **Accepted and armed.** The fences activated on public testnet-12 at DAA 750 on 2026-09-27
  (about 07:29 JST), with release `c3dbaee3c` and params fingerprint `dbbc9104…`.
* Date: 2026-09-26 (decided), 2026-09-27 (armed)
* Decided by: the operator. On 2026-09-26 the decision was to put every CRITICAL and HIGH post-launch
  fix into one fence, with V02 taken as option (a). At 17:05 that day the height moved from DAA 500 to
  DAA 750.
* Amends: [ADR-0152](0152-account-stake-staged-reserve-and-vested-rewards.md) (SW-8's seed and anchor;
  L-3's lock life; A-6's lock budget; B's maturity). It was written after the fact, to close the gap
  of fences with no ADR ([INVENTORY.md](INVENTORY.md), gaps).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L31: Decision

<a id="source-0155-testnet-12-flag-day-daa-1300"></a>

## 0155-testnet-12-flag-day-daa-1300.md

[原文](../adr/0155-testnet-12-flag-day-daa-1300.md) / [新しい主担当](0011-release-activation-and-flag-days.md)

原文 L5–L10

~~~~~~~~text
* Status: **Accepted, armed at DAA 1,300.** Release `587cab2b0`, params fingerprint `24e1aec3…`. The
  public fleet was upgraded one node at a time on 2026-09-27 between 19:10 and 19:37 JST.
* Date: 2026-09-27
* Decided by: the operator (2026-09-27, with the live chain at DAA ~853)
* Amends: [ADR-0152](0152-account-stake-staged-reserve-and-vested-rewards.md) (SW-10's refusal and SW-8's
  anchor-block void; L-3) and [ADR-0154](0154-testnet-12-flag-day-daa-750.md) D9.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L20: Decision

<a id="source-0160-claim-capacity-separation"></a>

## 0160-claim-capacity-separation.md

[原文](../adr/0160-claim-capacity-separation.md) / [新しい主担当](0035-bond-budget-and-shared-reservations.md)

原文 L5–L29

~~~~~~~~text
* Status: **PROPOSED v3 (2026-09-26 night).** Amends v2 (`93395bafa`, same file) in place, answering the
  second review of v2 (twelve findings; each is fixed or rebutted with evidence, Appendix D). The
  v2 → v3 and v1 → v2 changelogs are Appendix B; v1's summary is Appendix C. **Nothing in v3 rides the
  DAA-750 release.** Every consensus rule sits behind a t12-only dormant fence (§10), armed at one
  common height `H_cap` ∉ {500, 750, 1,000}, not a multiple of 250. The one node-only lane (shadow)
  ships first and moves no verdict.
* Date: 2026-09-26
* Base: public testnet-12 runs node update `8a0810992`, consensus params fingerprint `b8564b88…`.
  An unqualified `file:line` is at v2's base `40b23d64d` (= `rcore/cap-spec2`'s code, whose references
  the reviewer reproduced); code that exists only on the newer DAA-500 release line is cited as
  `rcore/int-4@a067f490d:path:line`; a lane reference is `branch@head:path:line`.
  Lane heads read for v3: weight `3aa4abec4`, escrow `d6a058249`, liab `deccda81b`, verify
  `29b1cb442`, shadow `151f48b0c` (the v1 fix-and-integrate workflow's current heads).
* Branch of this document: `rcore/cap-spec2` (worktree `~/Downloads/MISAKA-wt-b/wt-cap-spec2`).
  v1 lanes: `rcore/cap-{weight,escrow,liab,verify,shadow}`, integrating on `rcore/cap-int`.
  v3 lanes (§11): `rcore/cap-{d,q,s,b,n,m}`, each branched from `rcore/cap-int` after the v1 fixes merge.
* Numbers: every figure is reproduced by `docs/adr/0160-capacity/capcalc_v3.py`, whose output is
  committed as `docs/adr/0160-capacity/numbers-v3.md` (tables T1–T13, cited by name). v2's
  `capcalc_v2.py` / `numbers-v2.md` stay as history; where v3 and v2 differ, v3 governs.
* Related: ADR-0152 v3.1 (R-core+: SR, V, L, A, DA, §3.6 slash schedule, T-caps); ADR-0153 (2M
  attribution, unwritten); ADR-0043 (state root; amended by lane M's M-0b); ADR-0137/0144 (price of
  work); ADR-0142 (the DAA clock is a heartbeat cursor); the operator stopgap of 2026-09-26 01:00 (its
  anchor lane "lane A", `palw_operator_anchor_v1.rs`, and its operator-DA lane,
  `kaspad/src/palw_operator_da.rs`); the claim capacity map
  (`audit/claim-capacity` @ `c4e4a7c63`, `docs/palw-claim-capacity-map-2026-09-26.md`).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L172: 1.3 The user's decisions of 2026-09-26 17:50 JST (binding) / L416: 5.3 m — the credit is priced on refusal before payment (decision 2) / L1042: 11.1 Lane D — the decisions, the price record, AG-6 and the review's findings (rcore/cap-d) / L1231: 14. Decisions / L1233: 14.1 Earlier decisions, now / L1258: 14.2 Decisions for the user (each with a recommendation)

<a id="source-0161-a-claim-is-not-an-emission"></a>

## 0161-a-claim-is-not-an-emission.md

[原文](../adr/0161-a-claim-is-not-an-emission.md) / [新しい主担当](0036-capacity-scaling-and-emission-budget.md)

原文 L5–L11

~~~~~~~~text
* Status: **Proposed** (rcore/cap-s1, stage 5 of the user's capacity plan). Implemented behind the dormant
  fence F-M1 `palw_capacity_multi_claim` (testnet-12 only); armed nowhere.
* Date: 2026-09-27
* Decides: ADR-0160 v3 D-19 (emission at network ×N), option (b)+(c) together: riders share their block's
  carve, and the network's reward per DAA is the budget the attempt blocks' subsidies already are.
* Related: ADR-0160 §9.2–§9.3 (lane M), ADR-0126 (the carve at the lower DAA), ADR-0152 V-2/V-3 (withheld at
  acceptance, released at maturity, the coinbase identity), ADR-0058 (merged work).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L39: 3. Decision

<a id="source-0162-the-pair-opens-on-a-virtual-reserve"></a>

## 0162-the-pair-opens-on-a-virtual-reserve.md

[原文](../adr/0162-the-pair-opens-on-a-virtual-reserve.md) / [新しい主担当](0022-liquidity-seeding-and-virtual-reserves.md)

原文 L18–L33

~~~~~~~~text
* Status: PROPOSED 2026-10-01 at the operator's request; **IMPLEMENTED the same day** on
  `position/virtual-reserve` (§10), behind its own dormant fence `Params::palw_model_virtual_v1` —
  `None` on every preset, so no network's fingerprint, schedule or fold moves.
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decision 2 (the
  curve is over `X = V + reserve` again, and the market of a line exists from the line's creation);
  [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decisions 2, 3 and 5 (no seed opens a market; the seed is optional depth, taken before the first
  trade; `constants()`' third word is the virtual reserve again);
  [0094](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) (a pledge opens at the fence,
  with the pledge as its seed); [0120](0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md)
  (the least seed is zero past this fence); [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
  Decision 3 (the reward buys only a pair that trades); [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
  Decision 2 (the window's `market()` gains a twelfth word, `quoteSell`'s first word is the gross).
* Builds on: 0087 §0 (a position is not a share), 0088 (the line), 0114 (the fee schedules), 0152
  IMPL-15 (the buyback in a seat's lock price).
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L12: 2026-10-09 改定 — Positionと市場の対象を固定する / L92: 4. Decisions / L407: 2026-10-16 改定(ユーザー決定)— genesis・年次フォークのモデルは Position 化できない

<a id="source-0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed"></a>

## 0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md

[原文](../adr/0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md) / [新しい主担当](0016-artifact-ownership-and-model-lineage.md)

原文 L15–L20

~~~~~~~~text
* Status: PROPOSED 2026-10-03 (RFC-0001 §2.10, lane U); **IMPLEMENTED the same day on `rfc1/serve`**, behind its own
  dormant fence `Params::palw_adapter_class_v1` — `None` on every preset and in no testnet-12 flag-day list, so no network's
  fingerprint, schedule or fold moves.
* Builds on: RFC-0004 §6.3 (composite artifacts, PALW-MIP-15; `palw_improve_composite_v1`), RFC-0002 (the IR class, its
  registration and its court), ADR-0075 (a class is a consensus object).
* Supersedes nothing.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L37: 2. Decisions / L80: Mission alignment amendment — 2026-10-07

<a id="source-0165-the-floor-is-a-reserve-and-the-work-carries-the-clock"></a>

## 0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md

[原文](../adr/0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md) / [新しい主担当](0009-heartbeat-floor-and-liveness.md)

原文 L17–L17

~~~~~~~~text
**Status:** Direction amended **2026-10-07 (revision 5, user decision)**. Retain A″ (the idle-only bonded BASE-0 floor) and B (an attempt can carry a slot tick). Retain the independent bondless heartbeat lane of [ADR-0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) and [ADR-0140](0140-the-heartbeat-is-the-emergency-generator.md). **The proposed FALLBACK-V1 integration and heartbeat retirement are withdrawn**, including its added weight, bond envelope and per-bond slot credit; it is not the next fence. Sections 5-8 and 10.5 record this change.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L346: 3. How the rule got here (and A′, superseded) / L352: 5. FALLBACK-V1 integration - WITHDRAWN 2026-10-07 / L356: 6. Fallback accounting - WITHDRAWN 2026-10-07 / L434: Mission alignment amendment — 2026-10-07

<a id="source-0166-verifier-unavailability-is-not-producer-fraud"></a>

## 0166-verifier-unavailability-is-not-producer-fraud.md

[原文](../adr/0166-verifier-unavailability-is-not-producer-fraud.md) / [新しい主担当](0031-panel-independence-and-unavailability.md)

原文 L17–L21

~~~~~~~~text
* Status: PROPOSED 2026-10-03 at the operator's request; IMPLEMENTED the same day on `rcore/panel-liveness`, behind three
  dormant fences (`palw_panel_unavailable_expiry`, `palw_panel_fast_switch`, `palw_seat_availability`) — `None` on every preset,
  so no network's fingerprint, schedule or fold moves. Lane INT arms them in the DAA-5,300 list.
* Amends: ADR-0064 (silence is not observable — kept), ADR-0152 §S0′ (the second `ReceiptTimeout` / `NotReplayBacked` charge),
  ADR-0124 (seat draw weight), ADR-0160 E-4 (the obligation hold has no `PanelUnavailable` row).
~~~~~~~~

原文 L123–L123

~~~~~~~~text
**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §3（ADR166との接続）。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L106: Mission alignment amendment — 2026-10-07 / L115: MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定 / L121: 改定 — 2026-10-12（非不正終了と採掘枠の非早期回復：提案）

<a id="source-0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker"></a>

## 0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md

[原文](../adr/0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md) / [新しい主担当](0036-capacity-scaling-and-emission-budget.md)

原文 L17–L19

~~~~~~~~text
Status: implemented on `rcore/cap-1000` (testnet-12 only, int-11 release, H = 5,300). Supersedes nothing; completes stages 5, 6 and 7 of
ADR-0160 v3 (§8, §9, §11.4–11.6). Directive: the user, 2026-10-03 10:50 JST ("the claim ×1000 capacity change ships in the DAA-5,300
flag day release").
~~~~~~~~

判断・改定等を含む見出し（抽出）: L168: Mission alignment amendment — 2026-10-07 / L178: Bond予算・総影響保存の改定 — 2026-10-10

<a id="source-0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor"></a>

## 0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md

[原文](../adr/0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md) / [新しい主担当](0007-evm-exec-lane-and-anchors.md)

**定型のStatus宣言なし。本文から採否を推測して補完していません。**

判断・改定等を含む見出し（抽出）: L41: 2. Decisions / L181: 9. Status at the time of writing / L299: 10.7 RPC, status and open items

<a id="source-0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission"></a>

## 0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md

[原文](../adr/0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md) / [新しい主担当](0007-evm-exec-lane-and-anchors.md)

原文 L12–L12

~~~~~~~~text
Status: SUPERSEDED DIRECTION, 2026-10-08. Stable filename retained for existing links.
~~~~~~~~

<a id="source-0170-the-seed-anchor-is-a-window-not-a-span"></a>

## 0170-the-seed-anchor-is-a-window-not-a-span.md

[原文](../adr/0170-the-seed-anchor-is-a-window-not-a-span.md) / [新しい主担当](0028-attempt-ticket-and-seed-anchor.md)

原文 L15–L19

~~~~~~~~text
**Status:** PROPOSED 2026-10-03 on `anchor/window` (lane P2), for the DAA-5,300 flag day, the user's decision of the same evening
(variant "M1 + M2 + M3, W = 24"). Consensus change behind ONE fence, **dormant on every shipped preset**:
`Params::palw_anchor_window_v1`, a bare height whose companion values (`W`, `M1`) are hashed with it. Amends **ADR-0130** (what a seed
anchor is and how old a reader may take it) and **ADR-0147** (the jury's randomness and its population cut). Touches nothing else:
**SW-8's binding rule, ADR-0125's permit ledger, the audit period and its stagger, and every window of the panel are as they were.**
~~~~~~~~

判断・改定等を含む見出し（抽出）: L140: Mission alignment amendment — 2026-10-07

<a id="source-0171-probabilistic-constraint-checks-and-court-on-dispute"></a>

## 0171-probabilistic-constraint-checks-and-court-on-dispute.md

[原文](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md) / [新しい主担当](0033-probabilistic-checks-and-exact-court.md)

原文 L15–L15

~~~~~~~~text
**Status:** Design direction selected at the operator's request, 2026-10-06. Protocol specification/implementation pending; **not active on any network**. No activation height, implementation success or fingerprint change is implied by this document.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L21: Decision in Japanese / L27: 1. Why this decision / L110: Mission alignment amendment — 2026-10-07 / L120: Bond予算・総影響保存の改定 — 2026-10-10

<a id="source-0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm"></a>

## 0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md

[原文](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) / [新しい主担当](0014-versioned-kernels-and-backends.md)

原文 L15–L16

~~~~~~~~text
**Status:** Design direction selected by the operator, 2026-10-06. Documentation decision only;
implementation, cryptographic review and network activation are pending. No fence height assigned.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L189: Mission alignment amendment — 2026-10-07

<a id="source-0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation"></a>

## 0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md

[原文](../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L14–L19

~~~~~~~~text
**Status:** PROPOSED 2026-10-04 on `rcore/post5300-audit` (lane MU), the user's request after the DAA-5,300 release. Consensus change behind the
**same** dormant fence as the audit fixes, `Params::palw_audit_1004_v1` ("今回の監査修正で"): below it every path folds byte for byte as at
`0b1c11b87`, and on every shipped preset the fence is `None`. Amends **ADR-0088** (a version is in force from its publication — now: claimable from
its possession), **ADR-0135** Decision 4 (a readiness row is per class — now per root) and **RFC-0002 Part II Proposal A** (the seating floors are asked
of the claim's root). Touches **RFC-0004 §17.4.1** (a head moves only to an Active, seated class). Takes **no object tag**, **delta number 150**
(declared explicitly: `SeatRootReadiness`), **carriage tail `0xEB`**, no wRPC op and no DB prefix.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L134: Model acquisition outside consensus amendment — 2026-10-10後続改定 / L156: MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

<a id="source-0173-public-verifier-dispute-completeness-is-misaka-purpose"></a>

## 0173-public-verifier-dispute-completeness-is-misaka-purpose.md

[原文](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) / [新しい主担当](0038-g14-public-prosecution-and-completeness.md)

原文 L14–L21

~~~~~~~~text
* **Status:** Accepted as the project's design mandate at the user's request, 2026-10-07. Implementation and activation remain pending. This decision assigns no wire id, consensus fingerprint, activation height or deployment.
* **Purpose:** 正しい固定modelを保有/取得できたPanel外のpublic bondが、producer秘密状態なしにclaim固有の認証証拠から不正をlocalizeし客観的convictionへ到達する。モデル入手自体はchainが管理・保証しない（ADR177）。
* **Design path:** [RFC-0014](../rfc/0014-panel-independent-fraud-prosecution.md) specifies the implementation work and completion gates. [RFC-0015](../rfc/0015-panel-free-permissionless-verification.md) specifies a separately gated, deferred Panel=0 mode.
* **Complements:** [ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md)'s useful local-inference purpose, [ADR-0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md)'s verification direction and [ADR-0172](0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)'s kernel-only extension boundary.
* **Audit:** [All-RFC/ADR alignment report](../evidence/adr/0173-mission-alignment-audit-2026-10-07.md). The report records document versions, dispositions and the limits of this documentation change.
* **Amendment, 2026-10-10:** 旧Seeder/TRDC/FPR可用性方針は、後続の[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)で撤回した。従来の整合チェックは履歴で、新配分の経済/実装証拠ではない。
* **Further amendment, 2026-10-10:** 旧Seeder/TRDC/FPR可用性方針は、後続の[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)で撤回した。従来の整合チェックは履歴で、新配分の経済/実装証拠ではない。
* **Amendment, 2026-10-14 (user decision, [PRINCIPLES §0](../PRINCIPLES.md)):** MISAKAの**最終Goal**は「モデルを非公開にしておくことが経済的に不利になる仕組み」であり、不正の完全排除ではない。本ADRの公開検証者による不正追及の完結は、その下の制度要件として維持する: **登録モデルが1人にでも共有され、正しいPanel（正直な検証者）を建てられれば、claim固有の公開・認証済み証拠から不正を局所化し、客観的なcourtで立証してslashできる。** 誰にも共有されない閉鎖モデル（`p = 0`）の不正は排除を保証せず、ADR-0176のbond単位の上限で被害を有界化する残余リスクとする。検出確率の下限と期待利益が負であることは有効化gateではなく評価指標である。
~~~~~~~~

原文 L217–L217

~~~~~~~~text
**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §4。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L122: D9 — 永久不変のモデル同一性と入手への合意不介入（2026-10-10後続改定） / L135: D10 — モデル別拘束miner元本とcoinbase配分（2026-10-10後続改定） / L151: D11 — FPR撤回と条件付きpublic prosecution（2026-10-10後続改定） / L215: 改定 — 2026-10-12（Panel=1でもnon-seatの公開訴追を維持：提案） / L221: ユーザー方針 2026-10-15 による改定

<a id="source-0174-token-name-misaka-ticker-bili-address-prefix-unchanged"></a>

## 0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md

[原文](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md) / [新しい主担当](0001-network-identity-and-fingerprint.md)

原文 L12–L12

~~~~~~~~text
Status: Accepted naming and compatibility decision, 2026-10-07.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L14: Decision

<a id="source-0175-registered-models-are-permanently-immutable"></a>

## 0175-registered-models-are-permanently-immutable.md

[原文](../adr/0175-registered-models-are-permanently-immutable.md) / [新しい主担当](0015-permissionless-model-registration.md)

原文 L12–L12

~~~~~~~~text
**Status:** Accepted design, 2026-10-09. `pre`に独立fenceとfold/CLIを実装済み（休眠）。2026-10-10の[検証記録](../evidence/adr/0175-immutable-model-registration-2026-10-10.md)を参照。既存ネットワークの有効化高さは設定しない。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L63: 固定同一性・配布不介入と報酬配分の境界 — 2026-10-10後続改定 / L84: 改定対象と検証 / L90: MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

<a id="source-0176-bond-bounds-palw-production-rewards-and-final-weight"></a>

## 0176-bond-bounds-palw-production-rewards-and-final-weight.md

[原文](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) / [新しい主担当](0035-bond-budget-and-shared-reservations.md)

原文 L5–L8

~~~~~~~~text
* **Status:** Accepted as a design premise at the user's request, 2026-10-10. Specification details, implementation, economic measurements and activation remain pending.
* **Scope:** All future PALW reward-bearing producer paths, including Attempt, Free Prompt, claim-backed blocks, receipt rights, slices/riders/batches, unpaid escrow and settlement readers. Applies with existing Panels and with the separately gated Panel=0 mode.
* **Complements:** [ADR-0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md), [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md), [RFC14](../rfc/0014-panel-independent-fraud-prosecution.md) and [RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md).
* **Precedence:** For future PALW economics, this decision supersedes directions that increase bond-attributed economic or definitive consensus credit merely by multiplying claims. Earlier implementation records and old-claim rules remain historical. No wire id, Params field, activation DAA, fingerprint or deployment is assigned here.
~~~~~~~~

原文 L156–L156

~~~~~~~~text
**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §1・§3・§3A。
~~~~~~~~

原文 L166–L166

~~~~~~~~text
**状態:** 方針は確定（ユーザー）。コードは休眠実装・未build・未検証（`palw_bond_budget_v1` V3 policy、全presetで`None`、有効化は拒否）。数値は本番定数ではない。
~~~~~~~~

原文 L182–L182

~~~~~~~~text
**状態:** 判断は確定（ユーザー）。コードは新しい休眠fence **`palw_bond_formula_v2`** の背後に実装（全presetで`None`、Some-onlyで両fingerprintに入り、`Some(never())`は畳まれ、有効化は拒否）。各claimの受理DAAで判定するため、testnet-12で稼働中の式（`L_existing + L_new ≤ u·C`, `u = 0.5`, `L = ⌈PWU_exposure·S·A/ρ⌉ + m_c`）は変わらない（A-2一様）。値は名前付きfence定数であり本番承認値ではない。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L141: Model-bond allocationとの接続 — 2026-10-10後続改定 / L148: MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定 / L154: 改定 — 2026-10-12（共通原則の正本化、Bond/time台帳の具体化：提案） / L164: 改定 — 2026-10-15（ユーザー方針 2026-10-15: 共通拘束期間・上限の設定方法・測定手順） / L180: 改定 — 2026-10-15（ユーザー判断 2026-10-15: bond式の再設計） / L217: ユーザー方針 2026-10-15 による改定

<a id="source-0177-model-bond-allocation-without-availability-consensus"></a>

## 0177-model-bond-allocation-without-availability-consensus.md

[原文](../adr/0177-model-bond-allocation-without-availability-consensus.md) / [新しい主担当](0019-model-tiers-and-annual-catalog.md)

原文 L5–L10

~~~~~~~~text
* **Status:** Accepted design direction at the user's request, 2026-10-10, after the Seeder/FPR amendments. Allocation parameters, economic validation, implementation and activation remain pending.
* **Scope:** Future PALW model distribution and model-specific coinbase allocation, with Panels or the separately gated Panel=0 mode.
* **Supersedes:** MISAKA Torrent adoption/integration, dedicated Bonded Seeder roles/rewards, and the model-availability parts of [ADR173 D9–D11](0173-public-verifier-dispute-completeness-is-misaka-purpose.md), [RFC14 §16](../rfc/0014-panel-independent-fraud-prosecution.md), RFC15 and their related amendments. Mandatory Torrent, independent Bonded Seeder requirements, PoR/Full Fetch leases, TRDC/FPR model-disclosure gates and availability-based reward/weight suspension are withdrawn.
* **Preserves:** [ADR175](0175-registered-models-are-permanently-immutable.md)'s permanent model identity, [ADR176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)'s per-bond production/reward/Final-weight ceilings and common DAA hold, authenticated computation adjudication, existing monetary issuance constraints and old-claim rules.
* **2026-10-12 誤読防止:** 本ADRが外すのは**モデル配布**の合意管理であり、Miner・Panelのモデル保有義務と個別claimの検証・裁定・Final条件は外さない。[D1補足](#d1補足--役割の区別2026-10-12誤読防止)と[D7注記](#d7--非強制取得の下での検証経済安全性)を先に読むこと。Miner＋Panelのclaim bond合算と1.3〜2倍の配分係数は末尾の[2026-10-12改定](#改定--2026-10-12minerpanelのclaim-bond合算と1320倍の配分係数提案)（提案・未決）。
* **Implementation:** No code, wire id, Params field, activation DAA, fingerprint or deployment is assigned by this document. The earlier availability/FPR document checks record a withdrawn design, not current acceptance or economic evidence.
~~~~~~~~

原文 L206–L207

~~~~~~~~text
**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。
ユーザーの追加方針として、**モデルに実際に拘束されたMinerおよびPanelの適格claim bond元本の和**に応じた**1.3〜2倍の配分係数**を、モデル別の報酬配分（とブロック生成機会の配分）に反映する設計を提案として採録する。全文と攻撃試験C01〜C18は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §3A。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L12: 1. 採択する原則 / L184: 目的の改定 — 2026-10-10（bond集約優位） / L204: 改定 — 2026-10-12（Miner＋Panelのclaim bond合算と1.3〜2倍の配分係数：提案） / L229: 最終Goalと達成条件の改定 — 2026-10-14（ユーザー決定） / L260: ユーザー方針 2026-10-15 による改定 — 入手不能と合意creditの関係を変更

<a id="source-0178-fork-choice-heals-a-partition-on-bonded-participation"></a>

## 0178-fork-choice-heals-a-partition-on-bonded-participation.md

[原文](../adr/0178-fork-choice-heals-a-partition-on-bonded-participation.md) / [新しい主担当](0006-fork-choice-participation-and-settlement.md)

原文 L12–L16

~~~~~~~~text
**Status:** ACCEPTED — the user adopted rule E (2026-10-09). Implemented behind the dormant fence `palw_fork_choice_rule_e_v1`
(lane FINX successor, 2026-10-09; §9), refused when armed in this binary; it ships armed only in the single full-activation release,
which assigns its height. User decisions already taken (2026-10-08): no public advisory; no early dedicated fence; **the fix ships in
the single full-activation release**; `palw_dns_retirement_v1` may be armed only with rule E at or below it. Record:
[`docs/design/palw/finality-palw-consistency.md`](../design/palw/consensus/finality-palw-consistency.md).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L96: 4. Decision (ACCEPTED: rule E; as implemented) / L181: 8. Until the release (PROPOSED, node-local, no fence) / L264: 12. The 2026-10-10 drill stall is not this decision's problem (2026-10-10) / L273: ユーザー方針 2026-10-15 による改定

<a id="source-0179-model-tiers-genesis-annual-1-0-registered-0-3"></a>

## 0179-model-tiers-genesis-annual-1-0-registered-0-3.md

[原文](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) / [新しい主担当](0019-model-tiers-and-annual-catalog.md)

原文 L7–L9

~~~~~~~~text
* **Status:** Accepted(ユーザー決定 2026-10-15 第 4 回・第 5 回、[判断記録](../history/decisions/2026-10/user-decisions-2026-10-15-adr-rfc16-rfc17.md) §7b・§7c)。実装は休眠(fence `palw_model_tier_v1`、全 preset で `None`、有効化は承認していない)。build・試験は未実施で PASS ではない。
* **Supersedes:** [RFC-0017](../rfc/0017-model-eligibility-examination-and-renewal.md) のモデル資格審査(Registered → Probation → Eligible → Lapsed、抽選審査、更新、失効、試行手数料・試行上限)。RFC-0017 は本 ADR の階層制度の文書に改定した。
* **Related:** [ADR-0175](0175-registered-models-are-permanently-immutable.md)(登録の不変性)、[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)(配分 `α = 1.5`、30 日平均)、[RFC-0016](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)(V+)、[RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md)(Gate C)、[PRINCIPLES §8](../PRINCIPLES.md)(暗号学的な計算証明を使わない)。
~~~~~~~~

<a id="source-0180-palw-only-consensus-no-bft-validators"></a>

## 0180-palw-only-consensus-no-bft-validators.md

[原文](../adr/0180-palw-only-consensus-no-bft-validators.md) / [新しい主担当](0005-palw-consensus-architecture.md)

原文 L5–L11

~~~~~~~~text
* **Status:** Accepted(ユーザー決定 2026-10-16)。RFC-0012 の採用理由を記録する。
* **Related:**
  * [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md):PALW のみの合意と native EVM 決済。DNS validator・attestation・precommit・DNS-final・stake による reorg 拒否権を退役する。
  * fence `palw_dns_retirement_v1`:DNS-BFT の退役。RFC-0012 の方式 (b)。
  * [ADR-0178](0178-fork-choice-heals-a-partition-on-bonded-participation.md):bond 参加による fork choice。
  * [PRINCIPLES](../PRINCIPLES.md)。
* **Supersedes(合意の役割として):** ADR-0009〜0018 の DNS validator・確率的 finality・validator 報酬・stake attestation の系統。過去の記述は履歴として残す。
~~~~~~~~

<a id="source-0181-design-freeze-and-controlled-change"></a>

## 0181-design-freeze-and-controlled-change.md

[原文](../adr/0181-design-freeze-and-controlled-change.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L3–L8

~~~~~~~~text
- **Decision:** Accepted — 今回のユーザー指示。文書化日 2026-10-17。
- **Scope:** 設計・文書の変更管理。合意方式・既存の数値を変更しない。
- **Implementation:** 文書へ反映。node コードは対象外。
- **Verification:** [文書検査](../maintenance/validation-report.md)。node の検証ではない。
- **Activation:** 文書管理規程として適用。合意 fence の有効化なし。
- **Related:** [RFC-0021](../rfc/0021-design-freeze-and-documentation-governance.md)、[PRINCIPLES §11](../PRINCIPLES.md#design-freeze)。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L1: ADR-0181 — 採用済み設計を原則凍結し、重大問題時のみ新規 RFC・ADR で変更する

<a id="source-0182-defer-media-and-model-improvement-until-post-mainnet"></a>

## 0182-defer-media-and-model-improvement-until-post-mainnet.md

[原文](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L3–L10

~~~~~~~~text
- **Decision:** Accepted — 追加のユーザー指示。文書化日 2026-10-17。
- **Scope:** RFC-0003 の画像・動画経路、および RFC-0004 のモデル改善の開発優先度・初期提供・有効化時期。
- **Policy status:** DEFERRED_POST_MAINNET / DISABLED。
- **Implementation:** 方針を文書へ反映。コードの実際の無効化は未確認。
- **Verification:** 無効化・回帰試験は full checkout で実施が必要。今回 node 試験なし。
- **Activation:** 当面なし。mainnet 開始時にも自動有効化なし。
- **Related:** [RFC-0022](../rfc/0022-post-mainnet-media-and-model-improvement-scope.md)、[RFC-0003](../rfc/0003-palw-generative-model-classes.md)、[RFC-0004](../rfc/0004-palw-model-improvement.md)、[PRINCIPLES §12](../PRINCIPLES.md#post-mainnet-scope)。
- **Amends:** 上記機能を初期 mainnet / 次回の全面有効化の対象として扱う旧予定・完成条件。この範囲外の技術設計は置換しない。
~~~~~~~~

<a id="source-0183-g14-court-capacity-panel-floor-retention-detection"></a>

## 0183-g14-court-capacity-panel-floor-retention-detection.md

[原文](../adr/0183-g14-court-capacity-panel-floor-retention-detection.md) / [新しい主担当](0038-g14-public-prosecution-and-completeness.md)

原文 L3–L8

~~~~~~~~text
- **Decision:** Accepted — ユーザー決定 2026-10-16〜17 の実装記録(ADR-0181 の手続きによる記録)。
- **Scope:** G14 の裁定容量、Panel の最低担保、court を要する class の trace 保持期間、検出方針の claim gate。
- **Implementation:** branch `integ/g14-court-capacity-r2`(dormant fence。全 preset で None、t12 の identity は不変)。
- **Verification:** targeted test(`g14_court_capacity`、`g13_panel_liability`、`g14_detection_gate`、node `lg14a_legacy_filer_e2e`、`g14_kernel_route_e2e`)。全体 suite は未実行。
- **Activation:** なし。各 fence は測定記録と別途の有効化決定を要する。
- **Related:** [G14 完全性証明 §11](../audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md)、[ADR-0181](0181-design-freeze-and-controlled-change.md)。
~~~~~~~~

<a id="source-0184-native-liability-horizon-panel-record-and-single-certified-leg"></a>

## 0184-native-liability-horizon-panel-record-and-single-certified-leg.md

[原文](../adr/0184-native-liability-horizon-panel-record-and-single-certified-leg.md) / [新しい主担当](0027-claim-lifecycle-and-liability.md)

原文 L3–L8

~~~~~~~~text
- **Decision:** Accepted — ユーザー決定 2026-10-17(ADR-0181 の例外変更経路で記録)。
- **Scope:** RFC08(native claim の責任と手数料)、RFC07(Panel-licensed Final の event)、RFC18(認証済み報酬の支払い)。
- **Implementation:** RFC07 は `integ/validation-r2`(`KernelFinalReadV1::panel_record`、`panel_record_v1`)。RFC08 は 08-A(4 family の admission sourcing)と 08-B の native job v39(`palw_native_liability_horizon_v1`、kernel route aux 144)を `integ/validation-r2` に実装。text v29・funded v11 の保持と post-Final 告発は未実装。RFC18 は変更なし。
- **Verification:** core 単体のみ(保持・解放・返金・post-Final 有罪化、aux 144 codec、fingerprint 不変)。実ノード試験は未実施。
- **Activation:** なし(全 fence dormant、t12 identity 不変)。
- **Related:** [外部監査の状況](../audit/palw/design-reviews/rfc-completion-audit-2026-10-17-status.md)、ADR-0185。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L20: 実装記録 2026-10-18(設計変更なし)

<a id="source-0185-paid-lock-randomness-bootstrap-and-paid-sources"></a>

## 0185-paid-lock-randomness-bootstrap-and-paid-sources.md

[原文](../adr/0185-paid-lock-randomness-bootstrap-and-paid-sources.md) / [新しい主担当](0032-sampling-beacon-and-receipts.md)

原文 L3–L8

~~~~~~~~text
- **Decision:** Accepted — ユーザー決定 2026-10-17(F1・F2・bootstrap、ADR-0181 の例外変更経路で記録)。
- **Scope:** RFC-0007 paid claim(`palw_claim_challenge_v2`)の checker 抽選の乱数源と beacon source の範囲。PALW 合意方式・既存数値は変更しない。
- **Implementation:** `integ/validation-r2`(`8c5124c6a`、`7962446f3`)。`PalwClaimChallengePolicyV2::bootstrap_seed`(dormant、probe と全 preset で `None`)、`misaka_palw_challenge::paid_opt_in::collect_paid_seal_reveal_seed_v6`、`paid_claim_seal_anchor_v2` / `paid_claim_bootstrap_anchor_v2`、`palw_exec_v2_credit_approval_v1`。
- **Verification:** 単体試験のみ(challenge 6/6、core 41/41 と bootstrap 3/3)。実ノード試験・drill は未実施。
- **Activation:** なし。fence は全 preset で `None`、t12 identity 不変。
- **Related:** RFC-0007、[検証記録](../audit/palw/design-reviews/consolidated-validation-2026-10-15.md) §1・§4・§5、PRINCIPLES §8(乱数としての commitment / VRF は可、計算証明は不使用)。
~~~~~~~~

<a id="source-0186-hold-unresolved-liability-advance-independent-claims"></a>

## 0186-hold-unresolved-liability-advance-independent-claims.md

[原文](../adr/0186-hold-unresolved-liability-advance-independent-claims.md) / [新しい主担当](0048-claim-dependencies-and-prefix-origin.md)

原文 L3–L6

~~~~~~~~text
- **Decision:** Accepted — ユーザー指示 2026-10-17。
- **Scope:** RFC-0018 の共有 safe frontier と、それを読む決済・fork choice・pruning。
- **Implementation:** 段階 1・段階 2 とも休眠 fence(実装中)。
- **Related:** [RFC-0023](../rfc/0023-unresolved-claim-liability-and-independent-progress.md)、[RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md)、[ADR-0181](0181-design-freeze-and-controlled-change.md)(設計凍結の手続き)。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L25: 追記(2026-10-18):段階 1 の測定と RFC-0018 (ii)(iv) の変更

<a id="source-0187-g14-native-public-filing-and-post-final-mining-accusation"></a>

## 0187-g14-native-public-filing-and-post-final-mining-accusation.md

[原文](../adr/0187-g14-native-public-filing-and-post-final-mining-accusation.md) / [新しい主担当](0038-g14-public-prosecution-and-completeness.md)

原文 L3–L8

~~~~~~~~text
- **Decision:** Accepted — ユーザー決定 2026-10-17:G-N2 の実装値(Final 後の demand は生涯 8・同時 2、attestor の lock は seat-lock 関係を claim の reserved exposure に当てた額、責任期間は他 lane と同じ式)をそのまま採用する。最終の drill・測定で問題が出た場合は見直す。
- **Scope:** native funded v11・text29 の告発枠(G-N1)、owning39 mining claim の Final 後の追及(G-N2)、担保不足での有罪(G-14 / T-N13)。
- **Implementation:** branch `integ/g14-court-capacity-r2`。各 native lane の既存の休眠 fence の下(全 preset で `None`)。testnet-12 の identity は不変。
- **Verification:** v11 の node 試験 5 件、v39 mining court の fold 試験 4 件(G14 完全性証明 §13)。text29 と v39 の実 job の試験は harness が無く未実行。
- **Activation:** なし。各 native lane は測定記録と有効化の判断を要する。
- **Related:** [G14 完全性証明 §13](../audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md)、[ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md)、[ADR-0181](0181-design-freeze-and-controlled-change.md)、[RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md)。
~~~~~~~~

<a id="source-0188-d2-x3-arm-records-composition-and-pause-credit"></a>

## 0188-d2-x3-arm-records-composition-and-pause-credit.md

[原文](../adr/0188-d2-x3-arm-records-composition-and-pause-credit.md) / [新しい主担当](0038-g14-public-prosecution-and-completeness.md)

原文 L3–L14

~~~~~~~~text
- **Decision:** Accepted — ユーザー決定 2026-10-18 の内容を実装・記録する: X3 の期限は `prepare = window_bind + window_receipt`、`observe = window_challenge_at(activation)`、`resolve = B_disclose + B_court + Δ`（`Δ = B_cold_fetch + B_check + B_localize + B_carrier + B_reorg_slack`）、`hard = accepted + prepare + observe + resolve`。Δ の各項と `c_retain` は policy の名前付き入力で、0 は未測定として拒否する（既定値にしない）。
- **Scope:** `palw_court_model_scope_v1`（D2）と `palw_claim_hard_deadline_v1`（X3）の refusal。X3 の econ 接続（`C_upper` と beacon source の material 項）。X3 の DA pause 返還（`resume_claim_after_da_session_v2`）。D2 の readiness object の tag（230）。
- **Implementation:** branch `codex-free/d2-x3-finish`（起点 `release/t12-16000` 17cbdee9e）。
  - `PalwG14GatedFenceV1::{CourtModelScopeV1, ClaimHardDeadlineV1}` を追加し、無条件 refusal を `validate_palw_g14_fence_record_in_v1` の record gate に置き換えた。`PALW_G14_ARMING_RECORDS_V1` は**空のまま**（record を追加していない）。requirements は `PALW_D2_ARMING_REQUIREMENTS_V1` / `PALW_X3_ARMING_REQUIREMENTS_V1`。
  - X3 は `palw_x3_unmeasured_terms_v1` が空でない policy を record の有無に関係なく refuse する。spans は `palw_x3_composition_refusal_v1` で窓から読んだ合成と一致しなければ refuse する（activation で窓を読み直す）。
  - `palw_x3_composed_policy_v1` で合成 policy を作る。`candidate_from_horizons_v1` は未測定の B_* を 0 のまま残す（未承認の候補）。
  - X3 の claim には DA pause credit を与えない（`palw_x3_pause_credit_v1`）。旧 claim は pause をそのまま返す。
  - econ: `palw_minimum_price_c_upper_x3_v1`（`palw_econ_contracts_v1`）と `PalwBeaconSourceCostBoundsV1::minimum_gross_fee_x3`（`palw_beacon_source_fee_v1`）が retention の価格を `C_upper` と material 項に入れる。`c_retain = 0` で拡張が正なら UNKNOWN として refuse する。
- **Tag:** R-D2-READY は tag **230**（`PALW_A2_TAG_ALLOCATIONS_V1` に 2026-10-15 から予約済みの行）を確定した。kind の codec と A-2 kind-table の行は未着手。
- **Verification:** 下の「テスト結果」を参照。node-level の REQ-X3-1〜5 の block flow テストと、restart / reorg の replay 同一性は**未実施**（2026-10-18 の再確認でも未着手）。R-D2-COVER は 2026-10-18 に導出して試験を追加した（下の「R-D2-COVER」）。tag 230 の codec は実装せず、Proposed の選択肢を下に書いた。
- **Activation:** なし。`PALW_G14_ARMING_RECORDS_V1` は空で、D2 と X3 は全 real network で refuse される。X3 の policy 値（B_cold_fetch・B_check・B_localize・B_carrier・B_reorg_slack・c_retain）は**ユーザー承認と測定が必要で、未決**。
- **Related:** [mainnet-spec-register §3.1.1](../design/palw/integration/mainnet-spec-register-2026-10-12.md)、[RFC-0014 §7.2](../rfc/0014-panel-independent-fraud-prosecution.md)、[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)（D2 の出典）、[ADR-0187](0187-g14-native-public-filing-and-post-final-mining-accusation.md)。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L50: R-D2-READY — tag 230 の codec（Proposed、ユーザー判断待ち）

<a id="source-0189-tir-court-read-set-door-options"></a>

## 0189-tir-court-read-set-door-options.md

[原文](../adr/0189-tir-court-read-set-door-options.md) / [新しい主担当](0041-long-context-court-decomposition.md)

原文 L3–L6

~~~~~~~~text
- **Decision:** Proposed (2026-10-18). Nothing adopted, nothing implemented, no fence, no pin, no rule changed.
- **Scope:** the Huihui-Qwen3.5-2B 32K class, the cone term of the conviction door (`palw_tir_frame_cost_v1`, `cone_runs_bound_v1`).
- **Evidence:** [read-set-1018](../evidence/palw/g14-measure-2026-10-18/read-set-1018.md) (partial), [door-breakdown-1018](../evidence/palw/g14-measure-2026-10-18/door-breakdown-1018.md), [cone-runs-analysis-1018](../evidence/palw/g14-measure-2026-10-18/cone-runs-analysis-1018.md).
- **Related:** [ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md), [ADR-0181](0181-design-freeze-and-controlled-change.md), PRINCIPLES §11 (a structural change needs an RFC and an ADR and explicit adoption).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L1: ADR-0189 — TIR court read set and the conviction door: options (Proposed) / L30: Unmet conditions (why this is Proposed only)

<a id="source-0190-tir-door-sparse-fetch-designated-answers-tenure-split"></a>

## 0190-tir-door-sparse-fetch-designated-answers-tenure-split.md

[原文](../adr/0190-tir-door-sparse-fetch-designated-answers-tenure-split.md) / [新しい主担当](0041-long-context-court-decomposition.md)

原文 L3–L9

~~~~~~~~text
- **Decision:** Proposed (2026-10-18). Nothing adopted in code, no fence, no pin, no rule changed. Number taken: 0190 (next free in
  `docs/maintenance/number-registry.json`; 0188 is taken on `codex-free/d2-x3`, 0189 is the options ADR on this lane).
- **Scope:** Huihui-Qwen3.5-2B 32K class; the conviction door of the public court (`palw_tir_frame_cost_v1`).
- **Evidence:** [door-breakdown-1018](../evidence/palw/g14-measure-2026-10-18/door-breakdown-1018.md),
  [read-set-1018](../evidence/palw/g14-measure-2026-10-18/read-set-1018.md), [nonconone-door-options-1018](../evidence/palw/g14-measure-2026-10-18/nonconone-door-options-1018.md),
  and the structural window rule of `release/t12-16000` @ 484e8fa67 (`derived-upper-1018`, read only, not in this branch).
- **Related:** [ADR-0189](0189-tir-court-read-set-door-options.md), [ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md).
~~~~~~~~

判断・改定等を含む見出し（抽出）: L1: ADR-0190 — TIR conviction door: sparse fetch, designated answers, tenure split (Proposed) / L65: Decision for the next step (not adopted here) / L81: Chosen design (Proposed; not implemented)

<a id="source-0191-tir-dispute-local-decomposition-long-context"></a>

## 0191-tir-dispute-local-decomposition-long-context.md

[原文](../adr/0191-tir-dispute-local-decomposition-long-context.md) / [新しい主担当](0041-long-context-court-decomposition.md)

原文 L3–L13

~~~~~~~~text
- **Decision:** Proposed (2026-10-18). No fence is armed, no constant is pinned, no shipped rule changed. Step B (designated answers
  and tenure split) is implemented DORMANT behind a sibling fence in the same change; the bounded-round path of ADR-0190 is untouched.
- **Number:** 0191 (next free after 0190 in `docs/maintenance/number-registry.json`).
- **Scope:** every legal TIR job up to ctx 2,097,152 (graph-v7@2097152), not one 32K class. The 3,120 DAA door bound is kept, not raised.
  No class is excluded; 32K classes keep their IDs and semantics.
- **Related:** [ADR-0189](0189-tir-court-read-set-door-options.md), [ADR-0190](0190-tir-door-sparse-fetch-designated-answers-tenure-split.md),
  [ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md). Closure bound over 32K..2M: lane on `release/t12-16000`
  (`closure-bound-1018`); its numbers are to be substituted into the cone-free terms below when it lands.
- **Principles:** no cryptographic proof of computation is used (PRINCIPLES §8). A hash, a checkpoint commitment or a Merkle opening
  identifies data and authenticates it; it is never treated as evidence that the computation was correct. Correctness is established
  by a deterministic recomputation of ONE dispute, and by the public evidence that localises it.
~~~~~~~~

判断・改定等を含む見出し（抽出）: L1: ADR-0191 — TIR court: dispute-local decomposition for long-context claims (Proposed) / L178: Admission and the owner queue: derived (2026-10-19, supersedes the queue headroom above) / L243: Decision requested

<a id="source-0192-court-service-opportunity-and-shared-pursuit"></a>

## 0192-court-service-opportunity-and-shared-pursuit.md

[原文](../adr/0192-court-service-opportunity-and-shared-pursuit.md) / [新しい主担当](0042-court-service-and-versioned-clock.md)

原文 L3–L6

~~~~~~~~text
- **Decision:** Proposed / Review Required (2026-10-20)。採択・統合・有効化を意味しない。
- **Scope:** [RFC-0026](../rfc/0026-court-policy-master-contract.md) §4・5、監査 F02・F03。
- **Source:** `7fe073ec2`。別 branch の scheduler・tag 245 はこの snapshot に未統合。
- **Supersession:** 採択前は既存の期限・署名・責任・wire を置換しない。採択後も旧 claim は旧 ruleset。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L12: 判断が必要な理由 / L42: 採択後の受入条件

<a id="source-0193-miner-account-bond-for-panel-incentives"></a>

## 0193-miner-account-bond-for-panel-incentives.md

[原文](../adr/0193-miner-account-bond-for-panel-incentives.md) / [新しい主担当](0050-freeze-deferred-rejected-and-governance.md)

原文 L3–L9

~~~~~~~~text
- **Status:** Proposed / Review Required（未採択）。2026-10-20。
- **対応 RFC:** [RFC-0027](../rfc/0027-miner-account-bond-and-panel-incentives.md)。詳細契約・費用導出・移行・試験は同RFCを正本候補とする。
- **Owner / reviewers:** 未指定。設計責任者、経済、合意会計、検証/裁定、移行のレビューを要する。
- **Implementation / Verification / Activation:** 合意実装なし。数式検算と、旧node/外部検証の実測ログ再集計・shadow・合成資本実験を実行（§8）。新Account Bondのnode実測・有効化なし。
- **番号:** 添付ZIPではADR-0186までの本体を確認し、会話には未同梱のADR-0192への参照があるため、0193をローカル仮割当。他branchとの衝突確認は未実施。
- **採択者 / 採択日 / 有効化日:** 未記入。ユーザーの作成依頼を、この詳細案の採択または有効化と解釈しない。
- **通貨:** 本文のMSKは依頼時の旧ラベル。正式ticker BILIと同じnative coinであり、改名・単位変更ではない。[ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L38: D3 — 参加資格の新設を共通bond下限の変更で代用しない / L72: D8 — heartbeatを変えず、Miner実用性を採択前に測る / L84: D10 — 旧claimを守り、採択と有効化を分ける / L117: 6. 採択・検証・有効化の必要条件 / L144: 8.3 Panel報酬の判断

<a id="source-0193-sequential-witness-filing-clock"></a>

## 0193-sequential-witness-filing-clock.md

[原文](../adr/0193-sequential-witness-filing-clock.md) / [新しい主担当](0042-court-service-and-versioned-clock.md)

原文 L3–L3

~~~~~~~~text
Status: **Proposed / Review Required** (2026-10-20)。採択・統合・武装・配備ではない。RFC-0026 の補助案。正本は source snapshot `7fe073ec2:docs/rfc/0026-court-policy-master-contract.md` にあり、この古い X3 branch へ RFC 一式を統合したものではない。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L15: 候補実装と採択条件

<a id="source-0194-dormant-g14-integration-and-service-schema"></a>

## 0194-dormant-g14-integration-and-service-schema.md

[原文](../adr/0194-dormant-g14-integration-and-service-schema.md) / [新しい主担当](0042-court-service-and-versioned-clock.md)

原文 L3–L3

~~~~~~~~text
Status: **Proposed / Review Required** (2026-10-20)。利用者は A 案の隔離候補内での実装・検証を承認済み。公開 schema 採択・pre 統合・武装・配備は未承認。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L5: 判断の対象 / L64: 利用者の判断 — 2026-10-20

<a id="source-0195-rooted-court-response-eligibility-history"></a>

## 0195-rooted-court-response-eligibility-history.md

[原文](../adr/0195-rooted-court-response-eligibility-history.md) / [新しい主担当](0043-court-eligibility-and-initial-notice.md)

原文 L3–L6

~~~~~~~~text
- Status: **Proposed / Review Required** (2026-10-21)。本番 schema・責任規則・数値・有効化の採択ではない。
- Companion: [RFC-0028](../rfc/0028-rooted-court-response-eligibility-history.md)。
- Scope: RFC-0026 F02/F03 と、ADR-0192 A/A の block-start eligibility を実 state から検査するための保持情報。
- Candidate source: `codex/rfc14-g14-integration-candidate` / `11a9b2b0e`。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L32: 互換性と採択条件

<a id="source-0196-owner-independent-shared-tir-query-data"></a>

## 0196-owner-independent-shared-tir-query-data.md

[原文](../adr/0196-owner-independent-shared-tir-query-data.md) / [新しい主担当](0044-shared-tir-query-data.md)

原文 L3–L6

~~~~~~~~text
- Status: **Proposed / Review Required** (2026-10-21)。採択・武装・配備ではない。
- Companion: [RFC-0029](../rfc/0029-owner-independent-shared-tir-query-data.md)。
- Governing decision: [ADR-0192](0192-court-service-opportunity-and-shared-pursuit.md) A/A の詳細化指示と、[RFC-0026](../rfc/0026-court-policy-master-contract.md) F02/F03。
- Candidate base: `codex/rfc14-g14-integration-candidate` / `705b27616`。隔離候補内だけの実装・検証。
~~~~~~~~

<a id="source-0197-court-end-effects-and-pending-liability-archive"></a>

## 0197-court-end-effects-and-pending-liability-archive.md

[原文](../adr/0197-court-end-effects-and-pending-liability-archive.md) / [新しい主担当](0046-court-close-and-liability-archive.md)

原文 L3–L3

~~~~~~~~text
Status: **Proposed / Review Required** (2026-10-21)。利用者はAの詳細仕様化・隔離実装を指示。RFC-0030 §8のprivate adapterのみ部分実装し、全金融予約・qualified settlementは未実装。本番規則の採択・統合・有効化ではない。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L7: 必要な判断 / L29: 判断と採択の範囲 / L34: 利用者の判断 — 2026-10-21

<a id="source-0198-shared-court-initial-notice-boundary"></a>

## 0198-shared-court-initial-notice-boundary.md

[原文](../adr/0198-shared-court-initial-notice-boundary.md) / [新しい主担当](0043-court-eligibility-and-initial-notice.md)

原文 L3–L3

~~~~~~~~text
Status: **Proposed / Review Required** (2026-10-21)。利用者は2026-10-21にA（終端DAAを含む）の詳細仕様化・隔離実装を承認済み。本番採択・旧claim変更・統合・有効化ではない。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L5: 判断の背景 / L18: 判断の範囲

<a id="source-0199-shared-court-terminal-material-and-nonblocking-ingress"></a>

## 0199-shared-court-terminal-material-and-nonblocking-ingress.md

[原文](../adr/0199-shared-court-terminal-material-and-nonblocking-ingress.md) / [新しい主担当](0045-terminal-material-and-readiness.md)

原文 L3–L3

~~~~~~~~text
Status: **Proposed / Review Required**。Aの詳細仕様化・隔離実装を利用者が承認済み。本番規則は未採択。詳細は[RFC-0032](../rfc/0032-shared-court-terminal-material-and-nonblocking-ingress.md)。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L15: 判断範囲

<a id="source-0200-d2-readiness-root-opening-availability"></a>

## 0200-d2-readiness-root-opening-availability.md

[原文](../adr/0200-d2-readiness-root-opening-availability.md) / [新しい主担当](0045-terminal-material-and-readiness.md)

原文 L3–L3

~~~~~~~~text
Status: **Proposed / Review Required**。ADR-0188 Bの詳細codec選択は承認済み。本ADRはBの選択をやり直すものではなく、rootに隠れた偽digestを第三者が実際に開ける経路を決める。[RFC-0033](../rfc/0033-d2-readiness-root-opening-availability.md)に規約・検証対象を具体化した。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L18: 採択境界

<a id="source-0201-archived-close-qualified-financial-discharge"></a>

## 0201-archived-close-qualified-financial-discharge.md

[原文](../adr/0201-archived-close-qualified-financial-discharge.md) / [新しい主担当](0046-court-close-and-liability-archive.md)

原文 L3–L3

~~~~~~~~text
Status: **Proposed / Review Required** (2026-10-21)。詳細仕様化・隔離実装の選択待ち。[RFC-0034](../rfc/0034-archived-close-qualified-financial-discharge.md)に根拠、時限・署名・原資・金融writer・保持と受入条件を具体化した。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L20: 採択範囲

<a id="source-0202-court-original-funded-contract-and-role-consent"></a>

## 0202-court-original-funded-contract-and-role-consent.md

[原文](../adr/0202-court-original-funded-contract-and-role-consent.md) / [新しい主担当](0047-funded-original-and-role-authorization.md)

原文 L3–L3

~~~~~~~~text
Status: **Proposed / Review Required** (2026-10-21)。[RFC-0035](../rfc/0035-court-original-funded-contract-and-role-consent.md)に具体化。ユーザーはA（実UTXOで裏付けた独立escrow）の詳細仕様化・隔離実装を承認済み。金額・新料金・本番採択・pre統合・武装・配備の承認は含まない。
~~~~~~~~

<a id="source-0203-authenticated-palw-state-map-and-row-access"></a>

## 0203-authenticated-palw-state-map-and-row-access.md

[原文](../adr/0203-authenticated-palw-state-map-and-row-access.md) / [新しい主担当](0049-authenticated-state-map-and-row-stream.md)

原文 L3–L3

~~~~~~~~text
Status: **Proposed / Review Required** (2026-10-22)。[RFC-0036](../rfc/0036-authenticated-palw-state-map-and-row-access.md) に具体化。30 GB pruned node 方針（ユーザー決定 2026-10-22）の state lane。純ライブラリと dormant fence の実装のみ。本番採択・pre 統合・武装・配備・state root 形式の変更は承認されていない。
~~~~~~~~

<a id="source-0204-ibd-bounded-row-stream-and-storage-generations"></a>

## 0204-ibd-bounded-row-stream-and-storage-generations.md

[原文](../adr/0204-ibd-bounded-row-stream-and-storage-generations.md) / [新しい主担当](0049-authenticated-state-map-and-row-stream.md)

原文 L3–L5

~~~~~~~~text
Status: **Proposed / Review Required** (2026-10-22)。[RFC-0037](../rfc/0037-ibd-bounded-row-stream-and-storage-generations.md)。
設計: [ibd-generations.md](../design/palw/storage/ibd-generations.md)。設計所有者の明示的な採択まで、新 P2P message の
登録・protocol version の割当・有効化・配備は行わない。
~~~~~~~~

<a id="source-0205-a-decoded-answer-is-not-paid-twice"></a>

## 0205-a-decoded-answer-is-not-paid-twice.md

[原文](../adr/0205-a-decoded-answer-is-not-paid-twice.md) / [新しい主担当](0026-canonical-work-and-answer-accounting.md)

原文 L3–L5

~~~~~~~~text
Status: **Proposed / Draft** (2026-10-22, compute-claim audit lane compute-exec)。[RFC-0038](../rfc/0038-palw-fp-answer-accounting.md)。
監査: compute-exec CX-01 / CX-02。ADR-0145 §6 の「支払済み prefix は再び支払わない」の対象を prompt から
`prompt ‖ answer` に広げる。
~~~~~~~~

<a id="source-0206-panel-da-mandatory-disclosure-g14-route"></a>

## 0206-panel-da-mandatory-disclosure-g14-route.md

[原文](../adr/0206-panel-da-mandatory-disclosure-g14-route.md) / [新しい主担当](0039-data-availability-court-and-panel-da.md)

原文 L3–L7

~~~~~~~~text
- Status: **accepted**(2026-10-26 ユーザー決定: 期限付き必須公開と「期限まで非公開」への縮小を採用。値は目安のまま。fence は全 preset で `None`)
- Date: 2026-10-26
- Lane: g14-fam-panelda(branch `claude/g14-fam-panelda`)
- 関連: ADR-0077 D16 / SA-5、ADR-0119 D7、ADR-0192(claim 共通 frontier)、ADR-0198(notice a+3,120 / hard a+10,000)、2026-10-17 決定(court 保持 ≥ 4W+開示窓)、PRINCIPLES §8・§9
- ユーザー決定 2026-10-26:「PanelDa の実装は行なって 画像は実装しないけど」— FP V5(画像入力)には経路を作らない。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L38: 利用者への開示の文面(fence が governs する network で、初回利用前に gateway が表示)

<a id="source-0207-finite-fee-consent-and-provider-payment-bounds"></a>

## 0207-finite-fee-consent-and-provider-payment-bounds.md

[原文](../adr/0207-finite-fee-consent-and-provider-payment-bounds.md) / [新しい主担当](0037-fee-escrow-and-role-consent.md)

原文 L3–L3

~~~~~~~~text
Status: **Accepted fee scopes — 通常 verifier fee と provider fee-only 条件を採択** (2026-10-28)。provider の役務を有限化する別案（A/B）は未採択。ユーザーは「この報酬条件を採択して実装」と明示した。採択対象は元の有限 X3 期限までの全額保留、期限内の客観的有罪で元 poster へ全額返金、それ以外は期限後に verifier へ支払い（ServiceHold を含む）、遅い verdict による fee の遡及没収なし、既存 bond liability 維持。実装・検証・武装・配備の完了は意味しない。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L1: ADR-0207 — 通常 verifier fee と有限 provider 契約の判断 / L9: 採択済みの判断と未決の判断 / L24: Independent provider fee-only decision (Accepted, implementation pending) / L30: Provider fee-only 採択記録 — 2026-10-28

<a id="source-0208-authenticated-prefix-work-dependencies"></a>

## 0208-authenticated-prefix-work-dependencies.md

[原文](../adr/0208-authenticated-prefix-work-dependencies.md) / [新しい主担当](0048-claim-dependencies-and-prefix-origin.md)

原文 L3–L3

~~~~~~~~text
Status: **Accepted / 採択済み** (2026-10-28)。実装中、検証・有効化は未完。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L19: 採択記録

<a id="source-0209-genesis-bond-free-bootstrap"></a>

## 0209-genesis-bond-free-bootstrap.md

[原文](../adr/0209-genesis-bond-free-bootstrap.md) / [新しい主担当](0010-genesis-emission-and-bond-free-bootstrap.md)

原文 L3–L3

~~~~~~~~text
Status: **Accepted policy / 実装・検証未完**（2026-10-28、ユーザー決定）。仕様は [RFC-0041](../rfc/0041-genesis-bond-free-bootstrap.md)。
~~~~~~~~

判断・改定等を含む見出し（抽出）: L1: ADR-0209 — genesis bond を廃止し、BASE 回復と通常 bond 登録で起動する / L5: 判断
