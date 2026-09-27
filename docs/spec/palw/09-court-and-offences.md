# PALW spec — 09. Court and offences

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/court.md](../../design/palw/court.md). The code is the truth, and disagreements are
> listed in [divergences.md](divergences.md).

**Purpose.** A licensed claim can be disputed until it is `Final`, and in the ways chapter 08
allows, after Final while its vesting row is unmatured. A dispute narrows the committed execution to
one arithmetic step, or to two tiles of one decode row, that every node recomputes with the
canonical integer arithmetic. There is no vote and no tolerance. This chapter defines how a dispute
is opened, answered and narrowed, how it ends, and how a verdict becomes an offence. The amounts are
in chapter 10.

**Principles served:** §3 "ran as committed". P3: only the committed execution is on trial.

## 9.1 The adjudication contract

- **PALW-CT-11 (one step, canonical arithmetic).** A court verdict MUST be decided by recomputing one
  committed step (a tile) with the canonical reference arithmetic of chapter 04, reproduced by every
  node. It MUST NOT use a tolerance, a vote or challenge randomness. Within a pinned class, equality is
  full 64-byte equality. Across classes nothing is compared.
- **PALW-CT-12 (refutation first).** Direct refutation of a committed step is the primary path.
  Bisection (dissection) is the fallback when the challenger does not hold the step's inputs.
- **PALW-CT-13 (operands).** Operands are addressed in bytes under a pinned canonical encoding. The
  terminal adjudication is tile-local.
- **PALW-CT-14 (admission bounds the court).** A class MUST be admitted only if its own geometry bounds
  the court: every close fits `max_close_bytes` (80 KiB), checked by the ruleset's identity gate, and
  the class's ladder fits the minted ladder (§9.2). Coverage is over the reachable coordinates of the
  class, not over kernel ids.
- **PALW-CT-15 (the court is not optional).** A class carries weight only if every reachable step can
  be tried end to end (03 PALW weight gate). A fused class that no dissection can try is refused at
  admission (`palw_fused_dissectable`).
- **PALW-CT-16 (verdicts are terminal).** A proven verdict ends in a void or a reversed Final plus the
  slash of chapter 10. Freezing a class is permissive; slashing is strict.

**Sources:** ADR-0027 D1–D6, ADR-0049 A–E (C as amended 2026-08-26), ADR-0026 D3, ADR-0053 D1, ADR-0069,
ADR-0070, ADR-0093 D6. **Code:** `core/palw_court_v2.rs`, `core/palw_dispute.rs`, `core/palw_terminal.rs`,
`core/palw_e2e_adjudicability.rs`.

## 9.2 Dissection, the close and the ladder

- **PALW-CT-17 (k-ary dissection).** A dispute over a range narrows by k-ary dissection, with k derived
  from the move budget. Each rung MUST be answered within `court_turn_deadline` (42 DAA on testnet-12),
  and the whole court within `window_court` (3,000). An unanswered rung, or a close declaration that
  never assembles, is a **court default**. It is charged as a verdict and recorded `CourtDefault`
  (07 §7.7).
- **PALW-CT-18 (the close).** The bottom of a dissection is a close:
  - It opens tiles and is assembled from the executor's interval opening, whose close annex is served
    on the authenticated lane, together with the challenger's own replay.
  - A V4 interval opening carries the fold's digests and the Merkle frontier, never the range's leaf
    hashes.
  - The court addresses a block first, then a leaf.
  - A seat's recorded `Fault { leaf }` is a case it can prosecute.
- **PALW-CT-19 (fused attention).** A fused attention leaf is refuted by dissection over its history.
  The dissection's bottom proves the producer's own disclosure false, and it voids the claim
  `CourtHeldVerdict` past `palw_offence_attribution` (`palw_court_verdict_void_reason_v1`). Its moves
  are `CourtAttnRootClaimed(Anchored|Held)`, `CourtAttnDissected` and `CourtAttnChildChosen`.
  Responder coverage for the root claim is fenced (`palw_court_responder_coverage`).
- **PALW-CT-20 (the two-tile decode refutation).** For a tiled integer class, a decode token is
  challenged with `PalwTiledDecodePinV1`, which opens two tiles of the selecting row:
  - the tile that contains the committed token's lane;
  - the tile that contains a higher value.

  No claimed value rides. Both values are read from the opened tiles, so a forged argmax is refuted in
  O(tile + log vocabulary). The committed ids and the rows root are pinned by `PalwTiledDecodeTokensV1`.
  A conviction by this route is `ForgedOutputTiled` (contradiction 11). Under
  `palw_offence_attribution`, a `DecodeToken` or `DecodeTokenTiled` close is refused unless its
  narrowed leaf is the call's head slot. A decode arm's `NoFaultFound` is `DecodeCloseCannotAcquit`,
  never an acquittal.
- **PALW-CT-21 (the ladder).** The court ladder is minted once per ruleset, and a class's admission
  MUST fit it. A model too wide for the minted ladder is a new class on a new ruleset. The refutation
  walk reads `palw_court_ladder`, not `palw_context_ladder`, which is a separate fence.

**Sources:** ADR-0082 D1–D4, D6, D11; ADR-0085 D1–D4; ADR-0086 D1–D7; ADR-0092 D1–D5; ADR-0093 D1–D8;
ADR-0152 J-8. **Code:** `core/palw_bisect.rs`, `core/palw_court_deadline.rs`, `core/palw_step_refute.rs`
(`PalwTiledDecodePinV1`, `PalwTiledDecodeTokensV1`), `core/palw_attn_court_v1.rs`,
`core/palw_attn_dissect.rs`, `core/palw_attn_responder_v1.rs`, `core/palw_context_ladder.rs`.

## 9.3 One-move and special courts

- **PALW-CT-22 (one-move courts).** The shard court (`ShardCourtAccused`) and the checkpoint court
  (`CheckpointAccused`) decide in one move, with no session, and void `CourtFraud`. Under
  `palw_offence_attribution`, a one-move verdict goes through `palw_one_move_verdict_bound_v2`, which
  verifies the openings before it can return `NeedsDissection`. `NeedsDissection` is refused for
  classes whose root evidence cannot be built. Per-shard licensing is dormant on testnet-12.
- **PALW-CT-23 (held classes).** A held-context class is walked at its regime's ladder. The court reads
  the class's held context: root, opening, logarithm (04 §4.5). A held leaf the executor withholds is a
  DA default (08 §8.6), not a court verdict.
- **PALW-CT-24 (responder duty).** The executor's node MUST answer a dissection's root claim and every
  rung it owes (14 §14.1). Silence is a default, never an acquittal.

**Sources:** ADR-0100 D3 (the one-move court), ADR-0099 D5, ADR-0103, ADR-0119, ADR-0093, ADR-0152 J-8.
**Code:** `core/palw_shard_court_v1.rs`, `core/palw_checkpoint_court_v1.rs`, `core/palw_held_context_v1.rs`.

## 9.4 Offences and attribution (J)

Past `palw_offence_attribution` (testnet-12: DAA 0, genesis only):

- **PALW-CT-1 (offence kinds).** A consumed offence (`PalwConsumedOffenceV1`) has one of these kinds
  (`PalwOffenceKindV1`):

  | Kind | Name | Who writes it |
  | --: | --- | --- |
  | 0 | `ExecutorEquivocation` | filed (standalone) |
  | 1 | `PanelFalseValid` | refused past the fence (`SupersededOnThisNetwork`) |
  | 2 | `CourtExecutorGuilty` | refused as a contradiction (`ContradictionNotAdmitted`) |
  | 3 | `PanelFalseValidV2` | filed through `ObjectiveOffence` (tag 51) |
  | 4 | `ExecutorRefuted` | filed through `ObjectiveOffence` (tag 51) |
  | 5 | `DaDefault` | written by the DA sweep only. A filed one is refused ("never filed") |
  | 6 | `CourtConviction` | written by a proven court verdict only. A filed one is refused ("never filed") |

  Past `palw_rcore_plus` every record carries `collected` and `claim_id` (10 PALW-CO-26).
- **PALW-CT-2 (job identity and target).** A conviction MUST resolve its target with
  `palw_offence_target_v1`, trying in order the claim record, the liability record and the vesting
  row. The target yields `class_id`, `artifact_root`, `executor_bond`, `execution_root`, `lane`,
  `segment_count`, `phase`, `job_identity` and `trace_root`. No target means `NoTarget`.
- **PALW-CT-3 (the adjudicator).** A kind-3 conviction MUST pass
  `palw_check_panel_false_valid_v2`. The same function runs in the processor (with the signature) and
  in the fold (`consume_false_valid_v2`). The evidence is `PalwPanelFalseValidEvidenceV2`: a `Full`
  (V2) or `Segmented` (V3) receipt with verdict `Valid`, plus a contradiction. The check order:
  1. the byte cap;
  2. the version;
  3. an empty reporter slot;
  4. the accused is the receipt's seat, and the claim matches;
  5. the signature;
  6. the target;
  7. `ClaimUnderSession`, refused **only** when a court is open on the claim (an open DA session never
     blocks);
  8. the admission table;
  9. for execution-proving contradictions,
     `palw_panel_contradiction_convicts_execution_v1(…, target.execution_root, target.artifact_root,
     class ladder)`.

  It binds by root and never compares `claim_id`.
- **PALW-CT-4 (contradictions).** Admitted (`PalwPanelContradictionV1`): `ProducerWithholding` (2,
  only after a DA-confirmed default, never against `Sampled`, `Incapable` or `Unavailable` filers),
  `CourtFraud` (4), `StepArithmetic` (5), `StepStructural` (6), `ForgedOutput` (8),
  `IdentityMismatch` (9), `OutputMismatch` (10), `ForgedOutputTiled` (11), `LogitsNotStepOutput`
  (12), `PromptNotAnchored` (13). Refused by name: `ExecutorEquivocation`, `CourtExecutorGuilty`,
  `ConflictingPermit`, `Legs`.
  - A finding `acts_on_claim` for 5, 6 and 8–13.
  - Its forfeiture is **by root** for 5, 6, 8, 11 and 12, and **by claim** for 9, 10 and 13 (10
    PALW-CO-34).
- **PALW-CT-5 (liability by site).** A `Full` receipt is always liable. A `Segmented` receipt is liable
  for a `Whole` site only with a full mask, and for `Leaf(l)` only if its mask covers `l`'s segment.
  Otherwise the verdict is `SiteNotAttested` or `SegmentsUnknown`. The sites are defined in 08
  PALW-VF-16.
- **PALW-CT-6 (`ExecutorRefuted`).** A kind-4 conviction accuses `target.executor_bond` with a
  contradiction in {5, 6, 8, 9, 10, 11, 12, 13}. Its ledger key is one per claim. Its effect depends on
  the claim's phase:
  - live, with no open court: void `CourtFraud`;
  - Final: reverse the Final and mark the liability row;
  - voided or retired: record it and mark the row.
- **PALW-CT-7 (identity checks).** `palw_binding_identity_fault_v1` MUST refuse a binding that fails
  `verify_binding_v1`, whose root is not the target's, or whose `job_identity` is 0. It MUST report a
  fault on any of:
  - **J1**: the job id, or the FP job pin, differs from `job_identity`;
  - **J2**: the shape profile id differs from `class_id`;
  - **J3**: the execution seed differs from `job_identity[..32]`;
  - **J4**: the full-logits trace root differs from `trace_root`;
  - **J5**: the context differs from `CoreV1`'s (`palw_attempt_context_v1`) for the class's canonical
    job, or its prompt root differs, for prompts of at most 4,096 ids.

  Prompts above 4,096 ids are checked by `PromptNotAnchored` (13), metered against
  `PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1` = 2^18 ids per block.
- **PALW-CT-8 (admission under the fence).** A class registration MUST be refused unless:
  - its canonical job is `palw_attempt_canonical_v1(profile, false)`, that is `(n_ctx/8 − 1, 2)` for
    model classes;
  - `palw_logits_head_v1(profile)` exists;
  - it reaches no Kimi-K3 kernel;
  - a canonical prefill above 4,096 ids uses the `MerkleV1` prompt-ids form.

  03 §3.2 cross-references this rule.
- **PALW-CT-9 (a seat that found a lie).** A seat that finds a lie files it and files nothing else for
  that claim (0098). *Code:* `core/palw_false_valid_filing_v1.rs`.
- **PALW-CT-10 (slashing evidence must be genuine).** From DAA 750
  (`palw_slashing_evidence_utxo_genuine`), the UTXO side effect of a DNS slash MUST obey the same
  genuineness rule as the registry path. Without genuine evidence no stake UTXO is removed and no
  reporter is paid. The DNS slash itself is in `spec/dns-bft`. *Code:*
  `cons/pipeline/virtual_processor/utxo_validation.rs`.

**Sources:** ADR-0152 §3.10 J-1…J-8, with the audit session's `f2f1_spec.md` and its addendum
authoritative for names and discriminants (archive 0152/05); ADR-0098; ADR-0154 (MSK-26A).
**Code:** `core/palw_offence_v1.rs` (`PalwOffenceKindV1`), `core/palw_offence_attribution_v1.rs`
(`palw_offence_target_v1`, `palw_check_panel_false_valid_v2`, `palw_binding_identity_fault_v1`),
`core/palw_attempt_rules_v1.rs` (`CoreV1`).

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis (courts, attribution). From DAA 750: `palw_slashing_evidence_utxo_genuine`. `palw_shard_licensing` is dormant |

**Design:** `design/palw/court.md`.
