# PALW spec — 09. Court and offences

> **Partly written (Phase 2).** §9.4 is normative. §9.1–§9.3 are completed in the chapter 09 pass.
> [00-index.md](00-index.md) gives the conventions.

**Purpose.** A licensed claim can still be disputed until it is `Final`. A dispute bisects the
committed execution down to one arithmetic step (or two adjacent tiles) that any node can
recompute, and adjudicates that step. Fraud proofs are unilateral: no vote, and no challenge
randomness. This chapter defines how a dispute is filed, answered and bisected, how it terminates,
the special courts (attention, held context, fused rows, checkpoints, shards), and how a verdict is
attributed to an offence. Chapter 10 has the amounts that offences slash.

**Principles served:** §3 "ran as committed". P3: only the committed execution is on trial.

## 9.1 The adjudication contract

- [ ] What a court opens, and the bound that makes it independent of model size: operands addressed in
  bytes, terminal adjudication at tile level, admission that bounds the court (a registration
  obligation, chapter 03), coverage over reachable coordinates, and decode adjudicated by challenge.
  *Sources:* 0049 A–F (as amended 2026-08-26), 0053 D1 (the court is not optional), 0069/0070 (end
  to end). *Code:* `core/palw_court_v2.rs`, `core/palw_dispute.rs`, `core/palw_terminal.rs`.
- [ ] Unilateral fraud proofs are slash-terminal, with no BFT and no challenge randomness. *Sources:*
  0027.

## 9.2 Bisection and the close

- [ ] The k-ary court, and a close that is flat in the context. *Sources:* 0082 (court decisions),
  0080 (superseded in part). *Code:* `core/palw_bisect.rs`, `core/palw_court_deadline.rs`.
- [ ] The close is assembled from what the executor served (a disputed tile, not a capture). The
  interval opening carries the fold, not the leaves. *Sources:* 0085, 0086.
- [ ] **The two-tile refutation.** When the flat pin is inadmissible, the two-tile disclosure decides
  the step. Nothing vouches for the rows beyond the root. *Sources:* 0082, 0062 §(two-tile), 0093 §9.
  *Code:* `core/palw_step_refute.rs`.
- [ ] The court ladder is minted once, and the clock is what binds. Note the two fences:
  `palw_court_ladder` is what the refutation walk reads, and `palw_context_ladder` is a separate
  fence (ADR README, "Two labels"). *Sources:* 0092, 0084 U-08, 0077 Phase B. *Code:*
  `core/palw_context_ladder.rs`.

## 9.3 Special courts

- [ ] Attention is refuted by dissection. *Sources:* 0082. *Code:* `core/palw_attn_court_v1.rs`,
  `core/palw_attn_dissect.rs`, `core/palw_attn_responder_v1.rs`, fence `palw_attn_anchored_root`.
- [ ] Held context: a held class is walked at the regime's ladder. *Sources:* 0103, 0119, and 0152
  (the held-class court, A-held). *Code:* `core/palw_held_context_v1.rs`.
- [ ] Fused rows: the court can try a fused row, and the responder is a node duty (chapter 14).
  *Sources:* 0093. *Code:* fence `palw_court_responder_coverage`.
- [ ] Checkpoint courts. *Code:* `core/palw_checkpoint_court_v1.rs`. *Sources:* TODO, name the ADR.
- [ ] The shard court: the one-move court, and the licence per shard. *Sources:* 0099 D5, 0100.
  *Code:* `core/palw_shard_court_v1.rs`. Per-shard licensing is dormant on testnet-12.

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
