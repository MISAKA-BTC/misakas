# PALW spec — 17. The Model Improvement Protocol

> **Normative.** This chapter specifies RFC-0004, the PALW Model Improvement Protocol (as of
> 62fd7558c, with its amendment 8ad4c5dfa):
> - governed lines and their policy;
> - the epoch state machine and the material an epoch collects;
> - candidates and their composite artifacts;
> - evaluation, promotion, rollback, rewards and the pool.
>
> It applies past the dormant fence `palw_improvement_v1`. Below the fence nothing here is read, both
> of the protocol's tables are empty, and every root and carriage is byte-identical to a build
> without it. An engineer who reads only this file and chapter 04b must be able to build a second
> implementation of the transitions, the draw, the promotion rule and the pool that agrees with the
> fold on every input. Where the prose and a vector (§17.13) disagree, the disagreement is a defect of
> this text.
>
> Status: **Draft for review (A1)**.
> - §17.0 (the tags and ids every lane builds against) is final for v1.
> - The findings of the independent second implementation (`rfc4/ref2`,
>   `docs/design/palw/improve/ref2-findings.md`) are resolved here, each where its id is cited:
>   the RFC-era ones ([S1]…[S5], [P1]…[P9], [E1]…[E15]) and spec 17's own ([S6]…[S9], [P10],
>   [E16]…[E28]) and the differential's ([D1]…[D15]).
> - The coordinator's decisions are applied: E11, E12, P1, the head's place (RFC §3 amended), the
>   missing-evaluation rule, and — from the differential — E21/D2 (fees fund the draw; an abort refunds
>   each fee less its share of the spend), D15 (a setter missing any key loses its whole set and its
>   bond), D12 (vesting is final, on its own steps) and D5 (a missing guard score counts for the
>   incumbent, with its liveness cost).

Contents:
- §17.0 tags and ids;
- §17.1 conventions;
- §17.2 the fence;
- §17.3 state;
- §17.4 lines, the head and the policy;
- §17.5 the epoch state machine;
- §17.6 material and pools;
- §17.7 candidates;
- §17.8 evaluation;
- §17.9 promotion;
- §17.10 rollback;
- §17.11 the pool and rewards;
- §17.12 rules PALW-MIP-1…24;
- §17.13 vectors.

---

## 17.0 Tags and ids

Every number below was chosen free on every branch that touches the same space:
- `tir/fence-2`: object tag 67, delta 88–89, tails `0xC0`–`0xC1`;
- `rfc3/fp-v5`: object tags 68–69, delta 90, tail `0xC2`, court proofs 10–12, step fault 21;
- the capacity line: tails `0xB*`.

### Consensus objects (`PalwConsensusObjectV2`, appended)

Each variant's borsh discriminant is its tag. Tags 70–82 are appended after `CourtGenRootClaimed`
(69), so no earlier discriminant moves. An older build on a ruleset that declares
`palw_audit_2026_09_11` cannot decode them and skips them (A-2).

| Tag | Variant | Payload (`payload: Box<…>`) | Signed by | Payload module | Admission (lane) |
| --- | --- | --- | --- | --- | --- |
| 70 | `ModelLineImprovementPolicySet` | `PalwImprovementPolicySetV1` | the line's owner | `palw_improve_state_v1` | core (A2) |
| 71 | `HardCaseSubmitted` | `PalwHardCaseV1` | `submitter` (a bond) | `palw_improve_material_v1` | cand (A4) |
| 72 | `DataUseOptIn` | `PalwDataUseOptInV1` | the job's committer | `palw_improve_material_v1` | cand (A4) |
| 73 | `SetterSetCommitted` | `PalwSetterSetCommitmentV1` | `setter` (a bond) | `palw_improve_material_v1` | cand (A4) |
| 74 | `SetterSetRevealed` | `PalwSetterSetRevealV1` | unsigned (opens the commitment) | `palw_improve_material_v1` | cand (A4) |
| 75 | `SetterKeysRevealed` | `PalwSetterKeysRevealV1` | unsigned (opens the commitment) | `palw_improve_material_v1` | cand (A4) |
| 76 | `DatasetRegistered` | `PalwDatasetV1` | `registrant` (a bond) | `palw_improve_material_v1` | cand (A4) |
| 77 | `TeachingArtifactCommitted` | `PalwTeachingArtifactCommitV1` | `teacher` (a bond) | `palw_improve_material_v1` | cand (A4) |
| 78 | `TeachingArtifactRevealed` | `PalwTeachingArtifactV1` | unsigned (opens the commitment) | `palw_improve_material_v1` | cand (A4) |
| 79 | `TeacherLicenceRegistered` | `PalwTeacherLicenceV1` | the payload's `rights_holder_key` | `palw_improve_material_v1` | cand (A4) |
| 80 | `CandidateSubmitted` | `PalwCandidateSubmissionV1` | `submitter` (a bond) | `palw_improve_candidate_v1` | cand (A5) |
| 81 | `LineageHeadRolledBack` | `PalwLineageRollbackV1` | `filer`: the owner, or any bond with a proof | `palw_improve_state_v1` | core (A2) |
| 82 | `ImprovementPoolFunded` | `PalwImprovementPoolFundingV1` | unsigned (its carrier's sink output pays it, as `ModelBuy`) | `palw_improve_state_v1` | core (A9) |

Tags 83–85 are reserved for Phase F's pipeline-claim data-availability objects (the coordinator's
decision of 2026-09-29): evaluation claims otherwise have no on-chain DA transport, so a withholding
evaluator could not be convicted. Those objects are dropped by name below `palw_improvement_v1`. Tag 86
is `HardCaseKeyRevealed` (the material lane's: a hold-out case's committed key or reference, opened in
`Closing`, unsigned, as `SetterKeysRevealed` opens a set's). Tag 87 is the next free tag.

**Every layer asks the same three predicates:**
- the stateless gate (`palw_lifecycle_object_may_ride_v2`) lets each of the thirteen ride at every
  height;
- below the fence, the acceptance walk drops each one by name, first and charged nothing (an older
  build skips it), and the fold refuses it as the second lock (`ImprovementObjectRefused`);
- above the fence, an object whose admission has **landed** (`palw_improvement_object_landed_v1`) goes
  to its own arm in the object gate and in the fold; one whose admission has not landed is dropped and
  refused as below. The core lane's objects (70, 81) have landed; each lane adds its objects to that
  predicate as they land.

One function names them (`palw_improvement_object_name_v1`, with `palw_object_is_improvement_v1`). None
is an H-1 heartbeat carrier.

RFC-0004 §4.1's `LineageHeadSet` is not an object: the fold applies a promotion at scoring (§17.9).
Evaluation claims are the existing claim objects with the evaluation job family (A6).

### State

Headers are O(1); everything that grows lives in a keyed table (the Phase F reviewer's layout review of
75728b0b0). The thirteen tables, **in the order their collection roots enter the `improvement/v1` block**
(each collection root takes the table's name as its label):

| # | Table (label) | Key → row | Table id (delta 92) | Carriage tail |
| --- | --- | --- | --- | --- |
| 1 | `improvement_lines` | line → `PalwImprovementLineV1` (the header) | — (delta 91) | `0xC3` |
| 2 | `improvement_epochs` | (line, epoch) → `PalwImprovementEpochV1` (the header) | 1 | `0xC4` |
| 3 | `improvement_policies` | line → `PalwImprovementPolicyRecordV1` | 2 | `0xD0` |
| 4 | `improvement_usage` | line → `PalwImprovementUsageV1` | 3 | `0xD1` |
| 5 | `improvement_heads` | (line, seq) → `PalwLineageHeadEntryV1` | 4 | `0xD2` |
| 6 | `improvement_pools` | line → `PalwImprovementPoolV1` | 5 | `0xD3` |
| 7 | `improvement_material` | (line, epoch) → `PalwMaterialFrontierV1` | 6 | `0xD4` |
| 8 | `improvement_candidates` | (line, epoch, index) → `PalwEpochCandidateV1` | 7 | `0xD5` |
| 9 | `improvement_pool_entries` | (line, epoch, u8 kind, id) → `PalwPoolEntryV2` — kind 1 a hold-out case, 2 a setter set (§17.6.2) [D13] | 8 | `0xD6` |
| 10 | `improvement_items` | (line, epoch, item) → `PalwEvalItemV1` | 9 | `0xD7` |
| 11 | `improvement_results` | (line, epoch, item, subject) → `PalwEvalResultV1` | 10 | `0xD8` |
| 12 | `improvement_grants` | (line, epoch, index) → `PalwRewardGrantV1` | 11 | `0xD9` |
| 13 | `improvement_earnings` | bond → `u64` | 12 | `0xDA` |

| Item | Value |
| --- | --- |
| `state_root` block | label `improvement/v1`, then the thirteen collection roots in the order above; hashed only when any table holds a row (after `gen_classes/v1`). A later table appends its root after these, and moves no byte |
| Delta entry 91 | `PalwDeltaEntryV2::ImprovementLine { key, old, new }` (rows boxed) |
| Delta entry 92 | `PalwDeltaEntryV2::ImprovementRow { table, key, old, new }`: the key and the rows as their borsh bytes, `table` the id above. A table added later takes a table id, not a delta discriminant |
| Writers | `write_improvement_line` and one `write_improvement_*` per keyed table — the only writers |
| Carriage | one tail per table, encoded only when non-empty. Every row sits under its own key; every line has its policy, usage and pool; every detail row sits under an existing line, and (past the material, which may precede its epoch) an existing epoch |
| Indices (rebuilt, never hashed) | `improvement_due` (next_due_daa, line); `improvement_heads_of` class → lines; `improvement_retiring` (decided_daa, line, epoch); `improvement_open_epochs` |

**Reserved for the lanes' own tables** (their deltas are typed variants at these discriminants):

| Lane | Delta entries | Carriage tails | `state_root` block (after `improvement/v1`) |
| --- | --- | --- | --- |
| cand (A4: cases, setter sets, datasets, artifacts, licences, opt-ins) | 93–99 | `0xC5`–`0xCB` | `improvement-material/v1` |
| eval (A6: per-job state) | none: the job table is `ImprovementRow` table 13 (`improvement_eval_jobs`); 100–103 stay free | `0xCC` (`0xCD`–`0xCF` free) | `improvement-eval/v1` |

### Court and step ids (reserved, not used by step 0)

| Space | Reserved | Last used before |
| --- | --- | --- |
| `PalwCourtVerdictProofV2` tags | 13 `EvalCone`, 14 `EvalDecodeToken`, 15 `EvalDissection` (the evaluation lane's: an evaluation claim's court moves carry its `PalwEvalBindingV1` and composite parameter openings); 16 and up free | 12 (`GenDissection`, RFC-0003) |
| `PalwStepFaultV1` discriminants | 22 and up | 21 (`TirOutputDigestMismatch`, RFC-0003) |
| `PalwDaUnitV1` tags | 5–6 for Phase F's pipeline-claim units (step leaf, step node; an evaluation claim has no rows unit), 7 spare | 4 (`TirRowNode { level, index }`, the second IR fence's) |
| `PalwDaAnswerV1` tags | 7 and up for the pipeline-claim answers (the out-of-range proof is an answer of tag 5's shape, generic over the claim kind) | 6 (the row node's answer, after `TirStepOutOfRange` 5) |

Evaluation jobs are RFC-0003 pipeline jobs, adjudicated by the courts that already exist. A new proof
or fault is added only if A6/A7 show one is needed, and takes the next number here. Object tag 83 and
up, court proof 13 and up and step fault 22 and up are shared: a lane asks the core lane before taking
one.

### The fence

| Item | Value |
| --- | --- |
| Field | `Params::palw_improvement_v1: Option<PalwImprovementFenceV1>` (`palw_improve_v1`) |
| Value | `{ activation, scoring_set_id, sign_table_id, court_version, ceilings: PalwImprovementCeilingsV1 }` |
| `consensus_params_id` | Some-only: `"palw_improvement_v1/protocol-v1"`, the activation (LE u64), then the value |
| `consensus_schedule_id` | Some-only: `"palw_improvement_v1"`, the activation, then the value |
| Value bytes | `scoring_set_id` (64) ‖ `sign_table_id` (64) ‖ LE u16 `court_version` ‖ u8 `max_candidates_per_epoch` ‖ LE u32 `max_items_per_epoch` ‖ LE u64 `max_eval_positions_per_epoch` ‖ LE u16 `max_eval_budget_permille` ‖ LE u32 `max_governed_lines` ‖ LE u32 `max_policy_bytes` ‖ LE u32 `max_open_epochs` ‖ LE u32 `max_live_results` |
| Normaliser | `Some(never())` collapses to `None`; `for_each_fence` visits the activation only |
| Fork id | listed in `palw_fences_v1` as `("palw_improvement_v1", activation)` |
| Mirror | `PalwStateParamsV2::improve_from_daa` and `improve_ceilings` (borsh-skipped), written by `Params::sync_palw_improvement_v1` only |
| Drill | `--palw-drill-improve-at` (`palw_drill_improve_fence_at_v1`, list `PALW_DRILL_IMPROVE_FENCES_V1`, ceilings `PALW_DRILL_IMPROVE_CEILINGS_V1`) |
| Format caps | candidates ≤ 16, items ≤ 4,096 per epoch, positions ≤ 2^40 per epoch, budget ≤ 1,000 ‰, lines ≤ 1,024, policy ≤ 16,384 bytes, open epochs ≤ 64, **live results ≤ 2^17**; none zero. `max_live_results` is the product bound every block's root rehash pays: an epoch reserves `(n + regression_items + safety_items) × (k_max + 2)` result rows when it opens (the policy check refuses a policy whose epoch could never fit), and frees them when its results retire; an epoch opens only if the reservation fits. The drill's ceilings are (8, 1,024, 2^32, 500, 64, 16,384, 8, 2^14) |
| Prerequisites | a `ConsensusV2` ruleset (checked first); `palw_audit_2026_09_11` declared; `palw_tir_v1`, **`palw_tir_fence2`**, `palw_gen_v1` and `palw_kary_court` in force at or below the activation (the second IR fence since 2026-09-29: every evaluation pipeline sized under H7, and no verdict may flip mid-epoch). *An evaluation claim also needs `palw_fp_decode_rules` in force (§17.8.4); `validate_palw_v2` does not require it at arming in v1 **[decision A6-1]** — a ruleset that arms improvement without it opens epochs whose evaluation claims the header-context door refuses, so every such epoch ends `NoChange`. Recommended at the integration, with the decode-rules entry in a flag-day list: require it here as `palw_fp_job_v5` does.* |
| Shipped | `None` on every preset and in no testnet-12 flag-day list |

### Domain keys (keyed BLAKE2b-512, 64-byte output)

| Key | Used by |
| --- | --- |
| `misaka-palw/improve/scoring-set/v1` | `scoring_set_id` over the scoring library's descriptor (`misaka_palw_tir::scoring::scoring_set_descriptor_v1`, A7) |
| `misaka-palw/improve/sign-table/v1` | `sign_table_id` over the pinned binomial table (§17.9.2) |
| `misaka-palw/improve/policy/v1` | a policy's digest (§17.4.3) |
| `misaka-palw/improve/policy-set/v1` | the owner's policy message (§17.4.2) |
| `misaka-palw/improve/rollback/v1` | a rollback's message (§17.10) |
| `misaka-palw/improve/material-leaf/v1`, `…/material-node/v1`, `…/material-empty/v1`, `…/dataset-root/v1` | the material tree and `dataset_root` (§17.6.1) |
| `misaka-palw/improve/epoch-seed/v1` | the epoch seed (§17.8.1) |
| `misaka-palw/improve/draw/v1` | the draw order (§17.8.1) |
| `misaka-palw/improve/setter-item/v1`, `…/suite-item/v1` | a setter item's and a suite item's id (§17.8.1) |
| `misaka-palw/improve/judge/v1`, `…/pair-order/v1` | the judge draw and the pairwise order (§17.8.1) |
| `misaka-palw/improve/eval-seed/v1` | an item's generation seed, `H(epoch seed ‖ LE u32 item)` |
| `misaka-palw/improve/eval-job/v1` | an evaluation job's id (A6: `H(line ‖ LE u64 epoch ‖ LE u32 item ‖ borsh(subject) ‖ kind)`, with `borsh(subject)` = `0x00` (Parent), `0x01 ‖ class` (Candidate) or `0x02 ‖ class` (Previous), and `kind` one byte: ExactMatch 1, RefLogLik 2, Judge 3, Pairwise 4 [S9]) |
| `misaka-palw/improve/payout/v1` | an earnings payout's key, `H(borsh(bond) ‖ LE u64 DAA ‖ block)`, first byte forced to `0xFD` (§17.11.5) |
| `misaka-palw/improve/composite-artifact/v1` | a composite candidate's artifact root, `H(parent_class ‖ parent_root ‖ adapter_root ‖ LE u32 P)` |
| `misaka-palw/improve/candidate-declarations/v1` | a candidate's declarations digest |

ML-DSA-87 contexts: `misaka-palw-improve-policy-v1` (tag 70), `misaka-palw-improve-rollback-v1` (tag 81).
Improvement sink script: `OP_RETURN <b"MSKIMP01"> <64-byte line id>`.

---

## 17.1 Conventions

- **MUST**, **MUST NOT**, **MAY** are as in RFC 2119. All arithmetic is on integers; no value is ever a
  float.
- `H(key, x)` is keyed BLAKE2b-512 of `x` under `key`, with a 64-byte output. `‖` is concatenation. `LE
  u16/u32/u64` are little-endian encodings. `borsh(x)` is the borsh encoding.
- **DAA.** Every time in this chapter is a DAA score. A transition "at `t`" happens in the fold of the
  **first chain block whose DAA score is ≥ `t`**, during the block's step 2 (the pre-object sweeps),
  before any object of that block is applied [E3]. The acceptance rehearsal mirrors step 2
  (`palw_v2_pre_object_base_v1`), so an object is judged against the state step 3 sees.
- **Permille.** Every share is an integer in permille. A comparison against a share is made
  cross-multiplied, never rounded: "`b − c ≥ δ·n`" means `1000·(b − c) ≥ δ_permille·n` [P3].
- **Payouts** leave the fold as pending payout rows, which the coinbase pays, by the path the market's
  settlements use. Nothing in this chapter mints: every sompi a payout pays was first taken in, by a
  debit of a bond's collateral or by a sink (§17.11).

## 17.2 The fence

`palw_improvement_v1` is one fence with a value. Its shape and identity rules are in §17.0. Past its
activation, and only there, the fold reads this chapter. The fence's `ceilings` bound every policy
(§17.4.3), and the policy may only tighten them.

## 17.3 State

### 17.3.1 The line header (`improvement_lines[line_id]`, `PalwImprovementLineV1`) and its rows

| Field | Meaning |
| --- | --- |
| `line_id`, `class_id` | the spec 15 line, and its IR class at opt-in |
| `policy_digest`, `policy_sequence` | the digest of the policy in force, and the sequence of the last accepted policy object |
| `status` | `Governed`; `OptingOut { effective_daa }` (§17.4.4); `Dissolved` — the other rows are gone and the header stays, so the sequence continues |
| `governed_from_daa` | the DAA of the opt-in |
| `head`, `head_seq` | the head class, and how many head entries were ever appended |
| `next_epoch`, `open_epoch` | the next epoch number (from 1), and the epoch in progress, if any |
| `next_due_daa` | when the fold next advances the line (§17.5.2) |
| `barred` | at most 16 submitters barred after a rollback, each with the DAA its bar ends. An expired bar is pruned when the line is advanced. A new bar for a submitter already barred replaces it (removed, then appended). A 17th bar sorts the list by end, latest first (a stable sort), and keeps the first 16: the bar that ends soonest is dropped [D8/E19] |
| `last_promotion` | `PalwLastPromotionV1 { epoch: u64, owner_until_daa: u64, ban_daa: u64 }`: the latest promotion's epoch and its rollback terms, pinned at the promotion (§17.10.1) [E22]; kept while a rollback can name it |
| `regression_epoch`, `regression_check` | the epoch that ran the latest promotion's regression check (kept for its proof), and the predecessor still owed a check (§17.10.2) [E20] |

The line's other rows: `improvement_policies[line]` (the policy in force and a pending one),
`improvement_usage[line]` (`usage`, `since_daa`, §17.4.5), `improvement_heads[(line, seq)]` (the last 64
head changes), `improvement_pools[line]` (§17.11), and `improvement_material[(line, epoch)]` (each epoch's
material frontier — the next epoch's before it opens).

### 17.3.2 The epoch header (`improvement_epochs[(line_id, epoch)]`, `PalwImprovementEpochV1`) and its rows

| Field | Meaning |
| --- | --- |
| `state`, `times` | §17.5.1; `t_open`, `t_fix`, `t_close`, `t_draw`, `t_eval`, `t_score` (§17.5.3) |
| `parent`, `previous` | the head when the epoch opened; and the head's predecessor when this epoch runs the regression check (§17.10.2) |
| `policy_digest` | the policy the epoch runs under (a policy never changes inside an epoch) |
| `dataset_root` | fixed at `t_fix` (§17.6.1) |
| `candidates`, `pool_entries`, `holdout_cases`, `setter_sets`, `items`, `grants` | the counts of the epoch's keyed rows |
| `seed` | the epoch seed (§17.8.1) |
| `previous_counts`, `outcome` | the regression check's counts, and the decision (§17.9) |
| `escrow` | the parent's and the regression check's evaluation escrow, and what they spent (§17.11.2) |
| `decided_daa`, `retire` | when the epoch ended, and the retirement sweep's progress (§17.5.4) |

Its rows, keyed under `(line, epoch, …)`: `improvement_candidates` (in acceptance order, each with what
it paid and, once scored, its counts), `improvement_pool_entries` (hold-out cases and setter sets),
`improvement_items`, `improvement_results` (by item and subject) and `improvement_grants`.

Every table is written only past the fence, through its one writer, and every write is journaled. An
epoch's line MUST be governed (the carriage refuses anything else).

## 17.4 Lines, the head and the policy

### 17.4.1 The head (RFC-0004 §3, amended; open question 1)

A governed line's **head** is a class id held in its line row, with its history. The protocol writes
neither spec 15's version rows nor `current`; the market, the owner's leg, and the line class's share
and roots are untouched by it. (The RFC's first recommendation — the promoted class id as the current
version's root — broke three live readers of spec 15; RFC-0004 §3 records why.)

- At opt-in the head is the line's class (`PalwModelLineV1.class_id`). It MUST be an admitted IR
  class (`tir_classes`).
- A head change appends `PalwLineageHeadEntryV1 { epoch, class_id, previous, daa, cause }` to
  `head_history`. The cause is `OptIn`, `Promoted`, `RolledBackByOwner` or `RolledBackByProof`. The
  oldest entry is dropped past 64 entries.
- **The developer's promotion is refused** on a governed line: `ModelVersionPromoted`, and
  `ModelVersionPublished` with `preview = false` (which makes itself current), fail with
  `ImprovementLineGoverned`. Previews and withdrawals of previews are unaffected.
- A class MAY head several lines. A claim of it counts toward each such line's usage.

### 17.4.2 The policy object (tag 70)

```
PalwImprovementPolicySetV1 { line_id, sequence: u64, policy: Option<PalwImprovementPolicyV1> }
message = H("misaka-palw/improve/policy-set/v1",
            network_domain ‖ line_id ‖ LE u64 sequence ‖ (0x00 | 0x01 ‖ policy_digest))
```

It is signed with ML-DSA-87 under the context `misaka-palw-improve-policy-v1` by the key of the
spec 15 line's **owner** bond, at acceptance. A line without an owner (a genesis class's founding
line) cannot be governed. The acceptance layer verifies the signature; the fold checks everything else,
and refuses by name (`ImprovementPolicyRefused`):

- `sequence` MUST be the row's `policy_sequence + 1` (1 for a line with no row). An old signed policy
  cannot be replayed.
- **Opt in** (no row, `policy = Some`):
  - the line exists, is `Active` and has an owner;
  - its class is an IR class;
  - fewer than `ceilings.max_governed_lines` lines are governed;
  - the policy passes §17.4.3.
  The row is created: `head = class_id`, a history entry `OptIn`, `usage = 0`,
  `usage_since_daa = governed_from_daa =` this DAA, `next_epoch = 1`, and `next_due_daa` = the first
  grid boundary strictly after this DAA [E1].
- **Change** (row, `policy = Some`):
  - the line MUST still be governed at this DAA: past an opt-out's `effective_daa` a policy is
    refused until the line has dissolved, when it opts in afresh [D14];
  - with no epoch open, the policy is in force at once, and `next_due_daa` is recomputed on its grid
    (§17.5.2) [E28];
  - otherwise it becomes `pending_policy`, replacing any earlier pending one, and is in force from
    the epoch's end [E6];
  - an opt-out not yet in effect is cancelled: the line is `Governed` again.
- **Opt out** (row, `policy = None`): §17.4.4. An opt-out of a line already opting out is refused
  [D14].

### 17.4.3 The policy and its check

```
PalwImprovementPolicyV1 {                        // borsh, fields in this order [P10]
  version: u16 = 1,
  usage: { measure: u8 (Claims 1 | WorkLeaves 2), value: u128 },
  windows: { grid, w_collect, w_submit, w_holdout, w_eval, beacon_delay, court_margin: u64 },
  eval: {
    stages: Vec<{ kind: u8 (ExactMatch 1 | RefLogLik 2 | Judge 3 | Pairwise 4), params }>,
    regression_suite_root: [u8; 64], regression_items: u32,
    safety_suite_root: [u8; 64], safety_items: u32,
    judge_set: Vec<[u8; 64]>, anchor_floor_permille: u16,
    n: u32, n_min: u32,
    delta_permille, epsilon_permille, epsilon_safety_permille, alpha_permille: u16,
    max_new_tokens: u32, stop_ids: Vec<u32>, setter_cap_permille: u16, max_eval_positions: u64,
  },
  k_max: u8,
  fees: { registration_fee, candidate_bond, eval_fee_per_job, hard_case_fee, artifact_bond,
          setter_bond, dataset_bond, s1_bounty, s1_setter_reward: u64 },
  phi_permille, bounty_share_permille, promotion_share_permille,
  s2_trainer_permille, s2_dataset_cap_permille, s2_contributor_cap_permille: u16,
  provenance: { teacher_classes: u8, licence_classes: Vec<[u8; 64]>, full_weight_candidates: bool,
                base_licence_class: [u8; 64] },
  rollback_epochs, vest_epochs, ban_epochs: u32,
}
policy_digest = H("misaka-palw/improve/policy/v1", borsh(policy))
L_e = w_collect + w_submit + w_holdout + w_eval + court_margin        // the nominal epoch length
```

The encoding is borsh's [P10]:
- integers are little-endian at their width; a `bool` is one byte, 0 or 1;
- a `Vec` is `LE u32` length, then its elements; a 64-byte id is its bytes;
- `usage.measure` and a stage's `kind` are one byte holding the value shown;
- `params` is one byte of variant index — ExactMatch 0, RefLogLik 1, Judge 2, Pairwise 3 — then its
  fields;
- `teacher_classes` is a mask, class `c` (OpenDistill 1, LicensedDistill 2, SelfPlay 3, Human 4,
  ToolVerified 5, PublicData 6) at bit `c − 1`.

`policy.json` (§17.13) holds byte vectors of this form.

`PalwScoringParamsV1` is one of:
- `ExactMatch { open: i32, close: i32, key_cap: u32 }` (−1: no delimiter);
- `RefLogLik { logit_scale_q24: i32 }`;
- `Judge { lo: i32, hi: i32 }`;
- `Pairwise { margin: i32 }`.

The chain derives each stage's program from its kind, these parameters and the subject's shape, with
the builders `scoring_set_id` pins (A7). Two of the programs saturate, and the saturated value is the
score [D10, D11]:
- **RefLogLik** (`misaka_palw_tir::scoring::ref_loglik_v1`; `ref_loglik_reference_v1` is the same
  arithmetic in plain Rust): per logits row `r`, with `m = max_j x_j`, each scaled gap is
  `z_j = clamp((x_j − m)·logit_scale_q24, i32::MIN, 0)` — about −128 nats at the floor — and
  `log p(ref_r) = z_{ref_r} − IntLn(Σ_j IntExp(z_j))`. The value is the exact `i64` sum over the rows
  before `min(ref_count, rows_count)`, committed as `(hi, lo)`. A token the row puts more than
  128 nats below its best thus scores as if it were 128 nats below.
- **Judge** (`misaka_palw_tir::scoring::judge_v1`): the value is the judge's scalar clamped to
  `[lo, hi]`, so two outputs both past `hi` (or both below `lo`) tie.

A policy is refused by name unless every check below holds. `C` is the fence's `ceilings`.

| # | Check |
| --- | --- |
| 1 | `version = 1`; `borsh(policy)` is at most `C.max_policy_bytes` |
| 2 | `usage.value ≥ 1` |
| 3 | every window `≥ 1` and `≤ 2^32`; **`grid > L_e`** [E18]: an epoch opened at `g` ends by `t_score = g + L_e < g + grid`, strictly before the next boundary, which it would otherwise skip |
| 4 | **`w_eval > beacon_delay + PALW_IMPROVE_MIN_CLAIM_WINDOW_DAA_V1`**, where the constant is 32 DAA [E12]: the draw leaves at least 32 DAA of claiming |
| 5 | `1 ≤ \|stages\| ≤ 8`; each stage's params match its kind; at least one stage is primary (ExactMatch or RefLogLik); no kind appears twice |
| 6 | ExactMatch: `1 ≤ key_cap ≤ max_new_tokens`, `open ≥ −1`, `close ≥ −1`. RefLogLik: `logit_scale_q24 > 0`. Judge: `lo < hi`. Pairwise: `0 ≤ margin ≤ i32::MAX` (the stage's input interval) |
| 7 | a Judge or Pairwise stage requires `1 ≤ \|judge_set\| ≤ 16`, the set without duplicates, each an admitted IR class; without one, `judge_set` is empty; `anchor_floor_permille ≤ 1000` |
| 8 | `1 ≤ n_min ≤ n ≤ min(C.max_items_per_epoch, PALW_IMPROVE_SIGN_N_MAX_V1 = 2048)`, and `n + regression_items + safety_items ≤ C.max_items_per_epoch` (the ceiling bounds every item an epoch evaluates) [D9] |
| 9 | `regression_items, safety_items ≤ 1024`; a suite with items has a non-zero root, and a suite with none has a zero root |
| 10 | `delta_permille, epsilon_permille, epsilon_safety_permille ≤ 1000` |
| 11 | `alpha_permille` is one of the sign table's levels, `{10, 50}` [P1] |
| 12 | `1 ≤ max_new_tokens ≤ 4096`; `\|stop_ids\| ≤ 16`; `1 ≤ setter_cap_permille ≤ 1000` |
| 13 | `1 ≤ max_eval_positions ≤ C.max_eval_positions_per_epoch` |
| 14 | `1 ≤ k_max ≤ min(C.max_candidates_per_epoch, PALW_IMPROVE_SIGN_K_MAX_V1 = 64)` |
| 15 | `phi_permille, bounty_share_permille, promotion_share_permille, s2_trainer_permille ≤ 1000`; `1 ≤ s2_dataset_cap_permille, s2_contributor_cap_permille ≤ 1000` |
| 16 | `teacher_classes ≠ 0` and within the six defined bits; `\|licence_classes\| ≤ 32`, without duplicates; `base_licence_class ≠ 0` |
| 17 | `1 ≤ vest_epochs ≤ 64`; `rollback_epochs ≤ 16`; `ban_epochs ≤ 256` |

### 17.4.4 Opting out [E6]

A `policy = None` object with the row present sets `status = OptingOut { effective_daa }`:
- with no epoch open, `effective_daa = DAA + grid`;
- with one open, the status is set when the epoch ends, to `decided_daa + grid`.

While opting out, no epoch opens. From `effective_daa`:
- the line is no longer governed (the developer may promote again);
- its rows stay until every grant has vested or been forfeited and nothing is held;
- then the pool's balance goes to the spec 15 owner through the earnings ledger;
- then every row of the line is deleted but its header, which stays as `Dissolved` so the next opt-in
  continues its policy sequence (§17.11.5).

A policy object with `policy = Some` before `effective_daa` (or while the epoch that set it off is still
open) cancels the opt-out. Past `effective_daa` a policy is refused until the line has dissolved; a
second opt-out is refused at any time [D14]. A rollback stays valid until the line dissolves, governed
or not: its window and bar were pinned at the promotion, and its forfeits fund the pool whose balance
the owner receives at dissolution [E25].

### 17.4.5 Usage (the trigger's counter) [E1], open question 3

When a claim reaches `Final` and its class heads one or more governed lines, each such line's `usage`
grows:
- by 1 under `Claims`;
- by the claim's `pwu` under `WorkLeaves`.

This applies only if the claim was accepted at or after the line's `usage_since_daa`, and is not an
evaluation claim (A6 marks those). `usage` restarts at 0, with `usage_since_daa` = the DAA, when an
epoch opens and whenever the head changes.

## 17.5 The epoch state machine

### 17.5.1 States

```
                       grid boundary g, usage ≥ threshold
  (line Idle) ──────────────────────────────────────────► Open ──t_fix──► Submission ──t_close──► HoldOut
      ▲                                                                       │ no candidate          │ t_draw
      │                                                                       ▼                       ▼
      │ ◄───────────── Decided ◄──── scoring ◄── Closing ◄──t_eval── Evaluating ◄──draw── Drawing
      │                 (NoChange | Promoted)       (claims finalise)            (block ≥ t_draw + d)
```

| State (`PalwEpochStateV1`) | Value |
| --- | --- |
| `Open` | 1 |
| `Submission` | 2 |
| `HoldOut` | 3 |
| `Drawing` | 4 |
| `Evaluating` | 5 |
| `Closing` | 6 |
| `Decided` | 7 |

The line is `Idle` when `open_epoch = None`. `Vesting` is not a state: grants vest as records while the
line runs later epochs [E13]. `Drawing` [E3] and `Closing` [E4] name the two intervals the RFC left
unnamed.

### 17.5.2 Advancing a line

In step 2 of every block, the fold takes every line with `next_due_daa ≤ DAA`, in `(next_due_daa,
line_id)` order. It advances each one: it first vests the line's grants to this DAA (§17.11.4) [D12],
then applies every transition due at this DAA, in the order of §17.5.3, until none is due [E15], then
vests again. It sets `next_due_daa` to the smallest DAA, strictly after this one, at which any of these
becomes due:
- for an idle line: the next grid boundary strictly after this DAA;
- for an open epoch: its next time (every block while `Closing`, for an early Scoring);
- an opt-out's `effective_daa`;
- **the next vesting step** of any of the line's grants still vesting, `vest_from_daa + k·vest_unit_daa`
  for the smallest `k` that lands after this DAA [D12] — so a grant vests on its step even on a line
  with nothing else to do, and a rollback can never reach an amount that should already have vested.

The same computation runs after a rollback and after a policy that is in force at once [E28].

A block whose DAA jumps past several times therefore applies all of them, in order, in that block.

### 17.5.3 Transitions

Let `W` be the policy's windows. All times are fixed when the epoch opens.

1. **Opening.** At a grid boundary — the first block with `DAA ≥ g` for a multiple `g` of `grid`,
   taking `g` as the largest such multiple ≤ DAA — an idle, governed line with `usage ≥ usage.value`
   opens epoch `e = next_epoch`:
   - `t_open = g`, `t_fix = g + w_collect`, `t_close = t_fix + w_submit`, `t_draw = t_close + w_holdout`,
     `t_eval = t_draw + w_eval`, `t_score = t_eval + court_margin`;
   - `parent = head`, `previous = None` (the regression check joins at the draw, §17.10.2 [E20]);
   - `material` = the line's `pending_material`, which is reset;
   - `usage = 0`, `usage_since_daa = DAA`, `open_epoch = e`, `next_epoch = e + 1`.
   The S1 budget for the period is set (§17.11.3). A line opting out does not open. Otherwise the
   line's next check is the next boundary.
2. **`Open → Submission` at `t_fix`.** `dataset_root` is fixed over `material` (§17.6.1) and never
   changes [PALW-MIP-7].
3. **`Submission → HoldOut` at `t_close`.** The candidate set freezes; `K = |candidates|` from here on
   [P6]. With no candidate the epoch is **decided at once**: `NoChange(NoCandidate)` [E9].
4. **`HoldOut → Drawing` at `t_draw`.**
5. **`Drawing → Evaluating` at `t_draw + beacon_delay`.** The draw (§17.8.1): the epoch seed, the items
   and the judges. Then, in this order [E24]:
   - with fewer than `n_min` drawn items: **decided at once** `NoChange(TooFewItems)` [E9] (which
     items are primary is known only from their scores; scoring applies the same test to the primary
     count, §17.9.5);
   - the regression check joins if one is owed: `previous = regression_check`, if that class is still
     admitted (§17.10.2) [E20];
   - the parent's escrow and, with a check, `Previous`'s are reserved from the balance (§17.11.2); a
     balance too small for both: `NoChange(PoolInsufficient)`, the check still owed;
   - with the check reserved, `regression_check = None` and `regression_epoch = e`.
6. **`Evaluating → Closing` at `t_eval`.** No evaluation claim is accepted from here on.
7. **Scoring**, at the first DAA ≥ `t_eval` at which the evaluation lane reports every evaluation claim
   of the epoch final and the material lane reports no drawn set or case owing a reveal, and at the
   latest at `t_score` [E4]. `t_score` is the reveal deadline [E16]: a silent steward holds Scoring
   until then, never past it. Just before Scoring, the material lane (the core's before-scoring hook)
   settles every drawn set or case still owing a reveal: a setter that has not revealed **every** key
   of its set loses its **whole** set — every item of the set is dropped for every subject — and its
   setter bond is forfeited to the balance [D15]; a hold-out case owing its key or reference is
   dropped (§17.9.1). The
   fold computes the counts and the decision (§17.9), and applies it (§17.9.5, §17.11). The epoch is
   `Decided`.
8. **The epoch's end** (whenever an epoch becomes `Decided`, whatever the path):
   - `open_epoch = None`, `decided_daa = DAA`;
   - a pending policy comes into force;
   - a pending opt-out starts its delay;
   - unused escrows are returned (§17.11.2), the parent's and `Previous`'s first [D3];
   - the row is compacted: `items` and `results` are dropped, keeping `counts`, `outcome`, the
     candidates and the grants;
   - the line's next boundary check is the next grid boundary strictly after this DAA [E15].
   An epoch row is deleted once its grants are settled and no rollback can name it (§17.10).

A rollback while an epoch is open **aborts** it: `NoChange(Aborted)` (§17.10.3) [E11].

### 17.5.4 Retirement

A decided epoch's detail rows — its pool entries, its items and its results — are deleted by a bounded
sweep in step 2: at most `PALW_IMPROVE_RETIRE_ROWS_PER_BLOCK_V1 = 512` rows a block, oldest decided
epoch first (by `(decided_daa, line, epoch)`), and within an epoch pool entries, then items, then
results, each in key order. When none is left the header's `retire` is `Done`. The header, its
candidates and its grants are deleted when the line is next advanced after that, once every grant is
settled and the epoch is neither the line's `last_promotion` nor its `regression_epoch`. No block ever
journals a whole epoch's rows at once.

## 17.6 Material and pools

### 17.6.1 The material tree and `dataset_root` [E2]

The training material of an epoch is an append-only list of leaves. Each leaf is
`leaf = H("misaka-palw/improve/material-leaf/v1", u8 kind ‖ id)`, with these kinds:
- 1: a hard case (`case_id`);
- 2: a registered dataset (`dataset_id`);
- 3: a revealed teaching artifact (`output_hash`).

The tree is RFC 6962's Merkle tree over the leaves in admission order:
- `node = H("…/material-node/v1", left ‖ right)`;
- the empty tree's root is `H("…/material-empty/v1", "")`.

It is kept as a frontier (`PalwMaterialFrontierV1 { count, frontier }`, at most 32 hashes), so the root
of `count` leaves is computable at any time. `dataset_root = H("…/dataset-root/v1", LE u32 count ‖
root)`.

**Which list an item joins** depends only on the line's state when the item is admitted:

| When admitted | Hard case | Dataset, teaching artifact |
| --- | --- | --- |
| epoch `Open` | the epoch's material | the epoch's material |
| epoch `HoldOut` | the epoch's **evaluation pool** (§17.6.2) | the next epoch's material |
| any other time (line idle, or the epoch past `Open` and not in `HoldOut`) | the next epoch's material | the next epoch's material |

"The next epoch's material" is the line's `pending_material`, which an epoch takes when it opens. The
material lane (A4) admits each item and calls the core's one function for it
(`TransitionBuilder::note_improvement_material_v1`). That function returns where the item went, or a
refusal.

### 17.6.2 The evaluation pools

- **Hold-out cases.** A hard case admitted while the epoch is in `HoldOut` enters `holdout` with its
  submitter bond. At most `4·n` entries (`PALW_IMPROVE_HOLDOUT_FACTOR_V1 = 4`); a further case in
  `HoldOut` is refused by name.
- **Setter sets.** A `SetterSetCommitted` before `t_close` enters the pool as `{ set_id, setter,
  items }`, `1 ≤ items ≤ n`, at most 16 sets. Its items are `(set_id, i)` for `i < items`.
- **Keys** [D13/E27]. The pool is keyed `(line, epoch, kind, id)` — kind 1 a hold-out case (`case_id`),
  kind 2 a setter set (`set_id`) — so an id enters an epoch's pool once: a repeated case id or set id is
  refused by name. The draw's order keys are then distinct (its "ties are impossible"). In v1 setter items are
  generated only: an ExactMatch key, or an empty key for a judged item. Likelihood items come from
  hold-out cases with a continuation reference, disclosed from the draw by `HardCaseKeyRevealed` —
  one hash commits a set's keys, so a set cannot disclose a reference at the draw apart from its keys
  (PALW-MIP-9).

## 17.7 Candidates

`CandidateSubmitted` (tag 80) is admitted by the candidates lane (A5):
- the composite rule and the family rule;
- the declarations' form and references;
- admission of the class.

It is admitted only through the core's check `improvement_candidate_admissible_v1(line, epoch,
submitter, class_id, DAA)`, which refuses by name unless all of these hold:
- the epoch is in `Submission` (`t_fix ≤ DAA < t_close`) [PALW-MIP-8];
- fewer than `k_max` candidates are entered;
- the class is not the head, and not already entered in this epoch;
- the submitter is not barred at this DAA;
- the submitter bond's free collateral covers the candidate's payment (§17.11.1).

The candidate is written into the epoch row, in acceptance order, as `PalwEpochCandidateV1`, with what
it paid.

**No withdrawal** [E5]. v1 has no withdrawal object. A candidate is final from its acceptance, so
"nothing is withdrawn after `t_close`" holds trivially. At the epoch's end:
- losing candidates' bonds are refunded;
- the winner's bond vests with its grant;
- registration fees are the pool's balance from admission (an abort refunds each less its share of
  the spend it funded, §17.10.3).

## 17.8 Evaluation

### 17.8.1 The draw [E7], [E10]

At the block that moves the epoch to `Evaluating` (the first with `DAA ≥ t_draw + beacon_delay`):

```
seed        = H("misaka-palw/improve/epoch-seed/v1", block_hash ‖ line_id ‖ LE u64 epoch)
entry ids   : a hold-out case         → case_id
              setter item (s, i)       → H("…/setter-item/v1", set_id ‖ LE u32 i)
order key   = H("misaka-palw/improve/draw/v1", seed ‖ entry id)
```

`block_hash` is the hash of the chain block whose fold performs the draw — the chain's 64-byte block
hash (`Hash64`), as it is — and `line_id` is the line's 64-byte id [E26]. The block postdates the
hold-out, so the draw is fixed only after every candidate and every hold-out case is.

- **Draw.** The pool is sorted by `order key` (as bytes, ascending; ties are impossible). The items are
  the first entries of that order, with two exclusions:
  - an entry whose supplier (the case's submitter or the set's setter) already supplies
    `cap = max(1, ⌊κ·n / 1000⌋)` drawn items [κ = `setter_cap_permille`] is skipped;
  - the draw stops at `n` items.

  Items are without replacement. A pool smaller than `n` gives all of it [E10]. Drawn items are
  numbered `0..` in draw order.
- **Suites.** Then every regression item `j < regression_items` and every safety item
  `j < safety_items` is appended. Its id is `H("…/suite-item/v1", suite_root ‖ LE u32 j)` and its source
  is `Regression` or `Safety`. Suites are evaluated whole, every epoch [P4], and do not count toward `n`.
- **Per item.** `seed_i = H("…/eval-seed/v1", seed ‖ LE u32 i)`: one per item, the same for every
  subject.
- **Judges.** When the policy has a Judge or Pairwise stage, a judge is drawn for each drawn (non-suite)
  item: `judge_set[LE u64(H("…/judge/v1", seed ‖ LE u32 i)[0..8]) mod |judge_set|]`.
- **Pairwise order.** The order of a pairwise comparison of item `i` and candidate `C` is bit 0 of
  `H("…/pair-order/v1", seed ‖ LE u32 i ‖ C)[0]`, where `C` is the candidate's 64-byte class id (for
  `Previous`, the predecessor's class id) [S7]. It is the stage's `order` scalar: 0 shows the
  candidate first (as A), 1 shows the parent first. The Pairwise stage applies the order and the margin
  inside the circuit (`misaka_palw_tir::scoring::pairwise_v1`, A7).
- **Items' kinds.** An item is **primary** when its case's reference is an exact-match key or a
  likelihood reference; a judged-only item gives no primary outcome, does not count toward `n`, and
  feeds only the guards [P8]. The job family knows each item's kind from its case (A4, A6); the
  promotion rule reads it from the recorded scores (§17.9.1).

### 17.8.2 Subjects and jobs

The subjects of an epoch are:
- the parent;
- every candidate;
- in the regression check (§17.10.2), the head's predecessor, `Previous(class)`.

Every subject gets the same jobs on every item: one job per stage that applies to the item's kind. A
Pairwise job exists only for candidates and `Previous`: it compares the subject with the parent. The
job family is the evaluation lane's (A6):
- contexts derived by the chain;
- job ids `H("…/eval-job/v1", line ‖ LE u64 epoch ‖ LE u32 item ‖ borsh(subject) ‖ kind)`, `kind` one
  byte holding the kind's value (ExactMatch 1, RefLogLik 2, Judge 3, Pairwise 4 — the policy's
  encoding, not the scoring library's tags 0–3) [S9];
- open claiming in `[draw, t_eval)`;
- the fee per job, paid from the subject's escrow;
- capacity reservation (§17.8.4).

**Key disclosure** (RFC §7.1, amended):
- setter prompts, and the references of teacher-forced items, are disclosed at the draw;
- keys of generated items are disclosed only in `Closing` (`t_eval ≤ DAA < t_score`), and only when
  no evaluation claim on the set's (or case's) items is pending — every generation claim on them
  final or voided [E17]. Before `t_eval` a job not yet claimed has no claim, so "every claim final"
  would hold vacuously while the job is still claimable; `Closing` accepts no new claim (step 6);
- `t_score` is the deadline [E16]. A setter that has not revealed every key of its set by Scoring
  forfeits its setter bond and loses its whole set; a case owing its key drops (§17.5.3 step 7)
  [D15].

### 17.8.3 Results

The evaluation lane records each final score into the epoch row, through the core's one function
(`TransitionBuilder::record_improvement_score_v1`), as `PalwEvalResultV1 { item, subject, scores }`.
Each score is recorded by kind:

Each score is `PalwEvalScoreV1 { kind, value }`: the job id is derivable from `(line, epoch, item,
subject, kind)`, and the claim is the evaluation lane's job row's.

| Kind | `value` |
| --- | --- |
| ExactMatch | 1 pass, 0 fail |
| RefLogLik | the Q24 log-likelihood, `hi·2^31 + lo` of the stage's output |
| Judge | the judge's scalar |
| Pairwise | the stage's committed outcome from the candidate's side: +1 preferred beyond the margin, −1 the parent preferred beyond it, 0 otherwise |

A second score for the same `(item, subject, kind)` is refused. A score is taken only while the epoch
is `Evaluating` or `Closing`, for an item it drew and a subject it evaluates, and only inside its
kind's range [D6]: ExactMatch 0 or 1; Pairwise −1, 0 or 1; Judge inside a Judge stage's `[lo, hi]`;
RefLogLik any `i64`. A score outside it is refused by name, so the one outcome function (§17.9.1)
reads only legal values.

**A claim final after the epoch's end** [E23] is neither scored nor paid: the core's recorder refuses a
score, and its payer pays nothing, once the epoch is `Decided` (its unspent escrow has gone back,
§17.11.2). The evaluation lane skips both without failing the block. `court_margin` is the time a
court has to finish; an executor whose claim is still contested at `t_score` is not paid for it.

**ExactMatch is scored by the fold at the key's reveal**, not by a claim: keys stay hidden until every
subject's outputs are final, so no single claim may read one. An ExactMatch job is generation only; its
claim carries the generated ids, and the evaluation lane keeps the answer span's hash in its job row.
The arm that reveals a key (A4: `SetterKeysRevealed`, or a case's key) computes each subject's
pass/fail with the evaluation lane's `palw_improve_exact_match_score_v1` and records it here. RefLogLik,
Judge and Pairwise scores are recorded at `Final` from the claim's committed score lanes. The policy's
`key_cap` bounds a revealed key's length.

### 17.8.4 The evaluation claim (A6)

An evaluation job is claimed by a **free-prompt commitment** (subnetwork `0x4a`) whose job is **version 9**
[the coordinator's decision of 2026-09-29]; nothing new is registered and no object tag is taken. The rules
below are the evaluation lane's (`palw_improve_eval_v1`, `palw_improve_eval_fold_v1`).

**Carriage.**
- The job is an FP Job V4's bytes with the version word 9, then a `PalwEvalJobV1 { line_id, epoch, item,
  subject, kind, mode }` (the job's `tail`). Its fixed FP fields: public (`PublicDa`), its prompt on the
  payload (user mode), greedy (temperature 0, a zero seed), a zero nonce, canonical decode rules equal to the
  ones its mode derives (a generating job's stops and budget; a teacher-forced job's no-op rules), at most
  4,096 ids in its stream, its kind's mode, and the class its subject names.
- The payload is the FP payload's borsh, then `PalwEvalClaimTailV1 { generated: [u32], score: [i32],
  subject_layout, params }`. An FP decoder refuses the whole as undecodable, so a build below the fence
  refuses an evaluation claim at its door. The tail rides outside the signature and is bound by the roots:
  `trace_root` the step root, `output_root = H("misaka-palw/improve/eval-generated/v1", LE u32 n ‖ LE u32
  ids)`, `execution_root` over the job id, the class, the leaves, the step root, the prompt's root and length,
  the stage parameters, the generated root, the finalized outputs' root and the score
  (`palw_improve_eval_execution_root_v1`); `schedule_root` and `trace_manifest_root` are zero and
  `trace_chunk_count = 1`; the retention is the chain's (the accepting DAA plus the network's minimum). The
  claim id is the commitment's, its job named under `misaka-palw/improve/fp-eval/job-id/v1`.

**The door** — one decision at three places, so a build that carries the fence and one that does not agree
on every transaction below it:
- *Isolation* (height-free; mempool, block body and template): where the ruleset carries
  `palw_improvement_v1`, a version-9 payload goes to the evaluation door; everywhere else the FP door refuses
  its bytes as undecodable. The evaluation door decodes the payload and tail, runs **every rule of FP Job V4
  on the claim's stand-in** (the same payload with job version 7 and no tail: the network and key shape, the
  prompt ids against their hash and form, the context, the executed count and stop reason, the ladder, the
  ruleset's caps) under the ruleset's decode-rules door, then the claim's own stateless rules (the job's
  shape, the tail against the roots). Judged kinds are refused by name.
- *Header context* (the containing block's DAA): refused by name below `palw_improvement_v1`, below
  `palw_fp_decode_rules`, and past the structural work cap below the held regime (the FP lane's own height
  rules read an FP payload, so they never see this one).
- *The acceptance walk* (past `palw_improvement_v1` at the accepting block only): a version-9 payload becomes
  the `FreePromptCommitted` object its fold branch reads, after the FP walk's objects. The walk is total and
  skips, with its reason, whatever fails the stand-in at the claim's class's ladder and the ruleset's caps,
  the held regime's id rule, the claim's rules or the signature.

**The fold** refuses each of the following by name (`ImprovementRefused` unless another error is named), after
the free-prompt arm's bond checks (the bond exists and its key is the claim's, it is not retiring or frozen, and
it holds the producer floor):
- below `palw_improvement_v1`; no such epoch; the epoch not `Evaluating`; at or after `t_eval`;
- an item the epoch did not draw, or dropped; a subject the epoch does not evaluate; a Pairwise job of the parent;
- a class that is not the subject's; a kind the policy does not score; judged kinds (§17.8.4, below);
- a generating job whose seed, budget or stops are not the item's seed and the policy's;
- **a likelihood item** (a hold-out case whose reference is a continuation) **taking a generation job** —
  one job per stage that applies to the item's kind (§17.8.2); a teacher-forced job whose reference is not the
  case's disclosed one, or not yet disclosed; a prompt that is not the item's disclosed prompt, or not disclosed;
- stage parameters that are not the policy's; roots that are not the tail's; a committed score outside its
  kind's range (Judge inside the stage's `[lo, hi]`, Pairwise −1, 0 or 1);
- **the epoch's evaluation budget** (below); a class that is not an IR class; a layout that is not the one the
  class id binds; a `work_leaves` that is not the chain's closed-form count of the derived context;
- **a class the chain does not serve** (below);
- the job already taken by a live claim (open claiming, below); the executor's exposure ceiling
  (`FreePromptExposureCeiling`); the evaluation's share of the executor's room (below).

**Open claiming.** The first claim the chain holds takes the job. A claim voided or retired before `Final`
frees it — the row is cleared when the next claim for it is folded — so a dead claim never holds a job past
its own life.

**Capacity** (PALW-MIP-20; RFC-0004 §13, open question 7). Three bounds, all dormant below the fence:
- *Reservation.* The claim is an FP claim with no quanta, no `pwu`, no receipt rights, no weight and no reward
  escrow (PALW-MIP-17). It reserves what a claim of that work on its class reserves — its step leaves at the
  class's `slash_value_per_pwu`, under ADR-0160's stage-1 rule `⌈w / ρ⌉` — and passes the free-prompt arm's
  exposure ceiling, so a false claim is slashed like any other.
- *The epoch's evaluation budget in positions* **[decision A6-2]**. A job's positions are its prompt's ids plus
  its stream's. The budget is `B = min(policy.max_eval_positions, C.max_eval_positions_per_epoch ·
  C.max_eval_budget_permille / 1000)`, and the epoch's claimable jobs are at most `J = n·(1 + [Judge])·S +
  n·[Pairwise]·(S − 1)` over its `S` subjects: one job per drawn item and subject for the item's primary kind
  (an item is exact-key or likelihood, never both), one more for a Judge stage, and one per item and non-parent
  subject for a Pairwise stage. An ExactMatch key's scoring is the fold's, not a claim, and a suite's items are
  not claimable (below), so neither adds a job. A claim whose positions exceed `⌊B / J⌋` is refused. The shares
  are equal and independent, so no order of
  claims can spend another job's share, a voided claim returns nothing to anyone, and the epoch's total never
  exceeds `B`. *No chapter defines a claim capacity in positions; v1 takes the network's per-epoch ceiling as
  the span's capacity and `max_eval_budget_permille` as the evaluation's share of it — a network that wants
  the budget tighter lowers either ceiling. The RFC leaves the share open; this reading is the one the chain
  can compute.*
- *The share of the executor's room* **[decision A6-3]**. One evaluation claim may reserve at most
  `C.max_eval_budget_permille` of its executor bond's exposure ceiling (the room the bond carries), so an
  evaluation claim cannot take the room an executor's attempts need.

**Panels** (who seats an evaluation claim) **[decision A6-4]**. An evaluation claim is an FP claim of its
subject's class, so the chain derives its panel as it derives any claim's (the same seed, the same draw over
the seats ready for that class; no evaluation-specific code). The fold therefore requires the class to be one
the chain serves: not frozen (`FrozenClass`), admitted by the model registry where the registry governs
(`ClassNotAdmitting`), and verifiable inside the windows (`check_class_verify_admits_v1`, as a free-prompt claim
asks it: an NM class, an unmeasured deadline and a measured `D` over the cap are refused). Every seat that may
verify a candidate must hold its artifact; that is the registry's lifecycle for the candidate's class
(Prefetching → admitting), which an evaluation claim waits for. v1 pays the seats nothing for an evaluation
panel: the fee is the executor's (§17.11.2), and a claim without a reward has no panel share to credit.
*Open (coordinator): whether a share of `eval_fee_per_job` should credit the seats as ADR-0124 credits a
reward's, and whether evaluation claims should count in the class's replay room (ADR-0137): they do not in v1;
their load on the seats is bounded by the budget above and by each seat's own duty collateral.*

**Fees and scores at `Final`** (the claim's `finalize_claim`): the job's row records its finality; a kind that
committed a score (RefLogLik, Judge, Pairwise) records it through `record_improvement_score_v1`; the subject's
escrow pays `eval_fee_per_job` to the executor's bond as an earnings credit (§17.11.5), once. An ExactMatch
generation is scored at its key's reveal (§17.8.3). A claim final after the epoch stops taking scores earns
neither: the escrow is the epoch's, and returns with it.

**Not in v1.**
- *Judged kinds* (Judge, Pairwise) are not claimable **[decision A6-5]**: a judge class's kind (its output
  interface: a scalar head, a preference) is not specified, so the chain derives no judged pipeline and refuses
  every judged job by name. A policy that names a Judge or Pairwise stage still passes §17.4.3, but its judged
  items are never evaluated, and by §17.9.1 each counts for the incumbent: the guard then blocks every promotion.
  A line that wants promotions in v1 names ExactMatch or RefLogLik stages only.
- *Suite items* (regression and safety) have no disclosed prompt on chain — only the suite's root is — so no
  claim can evaluate them; each counts for the incumbent (§17.9.1), and a policy with `regression_items > 0`
  or `safety_items > 0` blocks every promotion until a suite's items are disclosed. *Open (spec): how.*

## 17.9 Promotion

### 17.9.1 Item outcomes [S5], the missing-evaluation rule

For candidate `C` (or `Previous`), the parent `H`, and item `i`, `outcome(i)` is Win, Loss or Tie.
Read the item's score of the kind in question for both subjects (`s_C`, `s_H`).

- **An item missing an evaluation counts for the incumbent.**
  - `s_H` missing: Loss (a parent win, against every candidate).
  - `s_C` missing: Loss.
- ExactMatch (pass 1 / fail 0), RefLogLik and Judge: Win if `s_C > s_H`, Loss if `s_C < s_H`, Tie
  otherwise.
- Pairwise: the recorded value is already the outcome (the stage applied the order and the margin):
  Win if it is +1, Loss if −1, Tie if 0. A8 does not apply the margin again. A missing value is a Loss.
- **Dropped items** — a setter item whose prompts or keys were never revealed, or a hold-out case whose
  committed key or reference was never revealed — are excluded from every count, for every subject: a
  supplier that never reveals cannot veto a promotion by making every candidate lose its item. The
  material lane drops them just before scoring (`drop_improvement_item_v1`) and forfeits a
  non-revealing setter's hold.
- The **primary** kind of a drawn item is read from the recorded scores: ExactMatch if any subject has
  an ExactMatch score on it, else RefLogLik if any has a RefLogLik score; a drawn item with neither but
  with a guard score is judged-only; a drawn item with no score at all is a primary item every subject
  misses, so it counts, and counts for the parent (a Loss for every candidate).
- **Regression and safety items are never judged-only** [S6]: each always counts in its suite (`N_r`,
  `N_s`), by its primary kind if any subject has one, else as ExactMatch — so a suite item without a
  primary score, whatever guard scores it has, is a Loss for every candidate.

There is **one** outcome function, the core's `palw_improve_item_outcome_v1` (in
`palw_improve_promotion_v1`), and every lane calls it [D6]. With the ranges §17.8.3 enforces, a
Pairwise value is exactly −1, 0 or 1.

### 17.9.2 The pinned sign table [P1], [P2]

`k(m, a)`, for `m` decisive items at level `a`, is the smallest `k ∈ 0..=m+1` with
`P[Bin(m, ½) ≥ k] ≤ a`. At level `a = α/(1000·K)` that is:

```
1000 · K · Σ_{i=k}^{m} C(m, i) ≤ α_permille · 2^m        (the empty sum, k = m + 1, is 0)
```

`k = m + 1` is the sentinel: no count attains the level, and the test cannot reject.

The table is a network constant. Its domain is:
- `m ∈ 0..=2048` (`PALW_IMPROVE_SIGN_N_MAX_V1`);
- `K ∈ 1..=64` (`PALW_IMPROVE_SIGN_K_MAX_V1`);
- `α ∈ {10, 50}` permille (1 % and 5 %, `PALW_IMPROVE_SIGN_ALPHAS_PERMILLE_V1`).

Its byte form is `b"PALW-IMPROVE-SIGN-TABLE-V1"` ‖ LE u16 2048 ‖ u8 64 ‖ u8 2 ‖ LE u16 10 ‖ LE u16 50,
followed by `LE u32 k(m, α/K)`:
- for each `α` in that order;
- then each `K` ascending;
- then each `m` ascending.

That is 262,272 entries. `sign_table_id = H("misaka-palw/improve/sign-table/v1", byte form)`, and the
fence's `sign_table_id` MUST equal it.

A line's `α` is one of the table's levels: the table decides, and the policy only selects [P1]. Every
count this chapter tests is at most `n ≤ 2048`, so the table always covers it. An implementation MAY
compute an entry from the definition. The digest is what binds it.

### 17.9.3 Counts

For each candidate `C` in the frozen set, over non-dropped items:

| Count | Over | `(b, c, t)` = wins, losses, ties |
| --- | --- | --- |
| `primary` | primary drawn items, by the item's primary kind | `n = b + c + t` |
| `regression` | regression items, primary kind | `N_r` = their number |
| `safety` | safety items, primary kind | `N_s` = their number |
| `judge` | every drawn item whose judge is not excluded, when the policy has a Judge stage — whether or not a Judge score was recorded [D5, S8] | |
| `pairwise` | every drawn item whose judge is not excluded, when the policy has a Pairwise stage — whether or not a Pairwise score was recorded [D5, S8] | |

**The guards count missing evaluations for the incumbent** [D5, S8]. A guard's count runs over every
non-dropped drawn item with a kept judge, and §17.9.1's rule decides each: a Judge item with either
side's score missing is a Loss; a Pairwise item without the candidate's recorded outcome is a Loss.
This is the conservative choice, and it has a liveness cost: a guard job nobody claims (a Judge or
Pairwise job, paid from the subject's escrow like any other) counts against the candidate, and enough
unclaimed guard jobs make the guard's sign test fire and block a promotion that the primary items
support. The other reading — counting only items that carry the guard's score — would let a candidate
escape a guard by leaving its guard jobs unclaimed. v1 takes safety over liveness: the escrow pays
for every guard job, so an honest executor is paid to claim it.

**Judge anchors [S4].** A judge is *excluded* for the epoch (its Judge and Pairwise scores void) when it
fails its anchors. The anchor pairs are, over the non-dropped drawn items it judged whose primary kind
is ExactMatch, every ordered pair of subjects `(p, f)` where `p` passed and `f` failed **and the judge
scored both** — a subject without both an ExactMatch and a Judge score on the item forms no pair [D4].
A pair is *correct* when the judge's Judge score of `p` exceeds its score of `f`. The judge is excluded
if:
- it has fewer than `PALW_IMPROVE_MIN_ANCHOR_PAIRS_V1 = 8` pairs; or
- `1000·correct < anchor_floor_permille · pairs`.

An unscored side is left out of the anchors rather than counted incorrect [D4]: counted incorrect, a
missing Judge score would let whoever withholds judge jobs push a judge under its floor and void its
guard, which is the incumbent's protection. A judge left under 8 pairs is still excluded, so a guard
rests on its anchor jobs being claimed: they are open to every executor and paid from the escrow. A
policy without a Judge stage has no anchors, and its Pairwise judges are never excluded.

### 17.9.4 Eligibility and the winner

`C` is **eligible** iff all of the following hold. `K` is the frozen set's size [P6], and `α` is the
policy's:

1. `n ≥ n_min`;
2. `1000·(b − c) ≥ δ·n` (`δ = delta_permille`);
3. `b ≥ k(b + c, α/K)`, the one-sided sign test at level `α/K` [P2];
4. regression: `1000·(c_r − b_r) ≤ ε·N_r` (true when `N_r = 0`) [P4];
5. safety: `1000·(c_s − b_s) ≤ ε_s·N_s`;
6. the judge guard holds: **not** `c_J ≥ k(b_J + c_J, α/K)`, the same test in the other direction
   [P5];
7. the pairwise guard holds: **not** `c_P ≥ k(b_P + c_P, α/K)`.

Guards are tested separately at `α/K`; a judge can block and never make a promotion.

The **winner** is the eligible candidate with the largest `b − c`. Ties go to the earliest in the
candidate set's acceptance order, which is the fold's order of `CandidateSubmitted` in the accepting
chain [P9]. Pipeline stages carry no weight other than primary or guard [P7].

### 17.9.5 The decision

`outcome` is one of:
- `Promoted { class_id, wins, losses }`;
- `NoChange { reason }`, where the reason is `NoCandidate` 1, `TooFewItems` 2, `NoneEligible` 3,
  `Aborted` 4 or `PoolInsufficient` 5.

`n` below `n_min` at scoring (after drops) is `TooFewItems`.

On `Promoted`:
- the head becomes the winner, with a history entry `Promoted` (whose `previous` is the parent), and
  `usage` restarts;
- the grants are made (§17.11.3);
- losing candidates' bonds are refunded.

On `NoChange`, every candidate's bond is refunded.

## 17.10 Rollback

### 17.10.1 The object (tag 81)

```
PalwLineageRollbackV1 { line_id, epoch, to_class, cause }
cause = Owner | CanaryFailed { claim, item } | LaterRegression { epoch } | LicenceViolation { challenge }
message = H("misaka-palw/improve/rollback/v1", network_domain ‖ borsh(payload))
```

It is signed with ML-DSA-87 under `misaka-palw-improve-rollback-v1` by the `filer` bond's key. Only
the **latest promotion** can be rolled back [E8]: the head's last history entry MUST be `Promoted` for
`epoch`, with `previous = to_class`, and the line's `last_promotion` MUST name `epoch`. A rollback of an
older promotion is refused: forfeiting its unvested rewards needs no rollback, and its class is no
longer the head.

**The terms are pinned at the promotion** [E22]. When an epoch promotes, the line records
`last_promotion = { epoch, owner_until_daa = DAA + rollback_epochs·L_e, ban_daa = ban_epochs·L_e }`
from that epoch's policy. The owner's window and the bar read these, never the policy in force at the
rollback: a submitter's exposure is fixed when its promotion is, and a later policy (which the owner
writes) can neither lengthen the window nor the bar.

A rollback is valid until the line dissolves, whether or not it is still governed [E25].

| Cause | Valid when | Head entry cause |
| --- | --- | --- |
| `Owner` | the filer is the spec 15 owner, and `DAA ≤ last_promotion.owner_until_daa` [E8, E22] | `RolledBackByOwner` |
| `LaterRegression { epoch: e2 }` | `e2 > epoch`, `e2` is decided, and its regression check shows `Previous` eligible against the parent with `K = 1` (§17.10.2) | `RolledBackByProof` |
| `CanaryFailed { claim, item }` | the evaluation lane records a canary job over `claim`'s final output that fails safety item `item`. **Not in v1 until A6 provides canary jobs**: refused by name | `RolledBackByProof` |
| `LicenceViolation { challenge }` | an upheld licence challenge. **Not in v1** (RFC open questions 10–11 give no challenge procedure): refused by name | `RolledBackByProof` |

### 17.10.2 The regression check

A promotion leaves a **regression check owed**: `regression_check = parent` (the head's
predecessor). The first epoch after it that reaches its draw evaluates the predecessor as the subject
`Previous(previous class)` [E20]:
- the check joins at the draw (§17.5.3 step 5), not at the opening, so an epoch that ends before its
  draw (`NoCandidate`) or at it (`TooFewItems`, `PoolInsufficient`) leaves it owed to the next — a
  quiet line does not lose the proof path of `LaterRegression`;
- it requires the predecessor to be still an admitted class at the draw (otherwise it is skipped and
  stays owed);
- once reserved, `regression_check = None` and `regression_epoch` names the epoch;
- a rollback or a new promotion replaces what is owed.

`Previous` runs the same jobs as a candidate, paid from the pool like the parent. Its counts
(`previous_counts`) are computed as a candidate's with `K = 1`. It is never a candidate: it cannot
win, and it does not count in `K`.

### 17.10.3 The effect

A valid rollback, in this order:
1. **vests** every grant of the line to this DAA (§17.11.4): vesting is final, and a vested amount is
   never taken back [D12];
2. **aborts an open epoch** [E11]: the epoch is `NoChange(Aborted)`. The parent's and `Previous`'s
   unspent escrow return to the balance; the evaluation spend already paid stays spent. Every
   candidate gets back its bond and its unspent escrow in full, and its registration fee **less its
   share of that spend** [D2/E21]: with `F` the sum of the epoch's fees and `S` the parent's and
   `Previous`'s spend, candidate `i` is refunded `fee_i − ⌈fee_i · min(F, S) / F⌉`, the rounding
   staying in the balance. (The fees were in the balance when the draw reserved those escrows, so they
   may have paid for them; refunding them whole could take the balance below zero, which MIP-21
   forbids.) A refund is never more than the balance holds — an S1 payout (§17.11.3) may have spent
   part of what the fees funded — taken candidate by candidate in acceptance order, so that a rollback
   is never refused for want of balance: it is the owner's safety valve [E21]. No re-basing and no
   waiting: the next epoch opens from the restored head under the usual trigger;
3. **forfeits** the unvested remainder, `amount − vested`, of **every** grant of the promoted epoch —
   the trainer's and every dataset's S2 grant and the winner's candidate bond — to the balance
   [PALW-MIP-16, D7];
4. **bars** the winner's submitter until `DAA + last_promotion.ban_daa` (`barred`, §17.3.1) [E22,
   D8/E19];
5. sets `head = to_class`, with a history entry, and restarts `usage`; the promotion can no longer be
   named (`last_promotion`, `regression_check` and `regression_epoch` are cleared); `next_due_daa` is
   recomputed (§17.5.2).

## 17.11 The pool and rewards

### 17.11.1 Money in

Three paths bring money into the pool.

1. **Bond debits.** An object signed by a bond that owes a fee or a bond amount debits the signer's
   collateral by that amount, as a slash does. The debited sompi are destroyed from the collateral.
   The pool records what it owes: a fee as balance, a bond as held until refunded or forfeited, an
   escrow as held for the jobs it pays. The debit is refused by name unless the bond is `Active` and
   its free collateral covers it. Free collateral is its collateral less slashed, less what the
   R-core+ ledger holds committed, less its accuser ledger.
   - `CandidateSubmitted`: `registration_fee` (balance, at once — the draw's escrow may use it
     [D2]), `candidate_bond` (held), and `eval_fee_per_job × J` (the candidate's escrow, held). `J`
     is the most jobs a subject can be given [D1]: one per item for its primary kind —
     `n + regression_items + safety_items`, since an ExactMatch job generates and the fold scores it
     at the key's reveal, and a likelihood job is one teacher-forced pipeline — plus `n` with a Judge
     stage (drawn items only), plus `n` with a Pairwise stage for a candidate or `Previous` (the parent
     has no Pairwise job). `palw_improvement_jobs_per_subject_v1` computes it.
   - `HardCaseSubmitted`: `hard_case_fee` (balance).
   - `TeachingArtifactCommitted`: `artifact_bond` (held).
   - `SetterSetCommitted`: `setter_bond` (held).
   - `DatasetRegistered`: `dataset_bond` (held).
2. **Sponsor deposits** (tag 82). The carrier pays exactly `amount` to the improvement sink
   `OP_RETURN OP_DATA8 "MSKIMP01" OP_DATA64 <line_id>` at `sink_index`, bound as `ModelBuy` binds its
   sink. The sink has two doors, as the activation sink has: isolation admits the output class on a
   ruleset that declares `palw_improvement_v1` and refuses any sink not bound to its carrier's
   `ImprovementPoolFunded` (index, line, value) or on a carrier paying no P2PKH-ML-DSA-87 output; the
   header context refuses the sink below the fence's height. The fold credits a governed line's
   balance; a deposit to a line that is not governed is refused, and P-B1's refund route pays it back
   to the carrier's first P2PKH-ML-DSA-87 output (also when the walk drops it).
3. **φ** [RFC §8.5]. Where spec 15 pays a governed line's owner leg (`split_owner_leg_v1`'s owner
   part), `⌊owner_part · φ / 1000⌋` goes to the pool's balance and the rest to the owner.

### 17.11.2 Escrow

At the draw, the parent's escrow (`J_parent × eval_fee_per_job`) is moved from the balance to the
epoch's escrow, and so is the `Previous` subject's. If the balance cannot cover both, the epoch ends
`NoChange(PoolInsufficient)`.

The evaluation lane pays each executor `eval_fee_per_job` from the subject's escrow, as a payout, when
the job's claim is final — while the epoch is `Evaluating` or `Closing`; a claim final after the
epoch's end is not paid [E23].

At the epoch's end, whatever is left of an escrow goes back to where it came from, **first** the
parent's and `Previous`'s to the balance — so S2's `R` (§17.11.3) is taken from a balance that
includes them [D3] — then each candidate's to its submitter.

### 17.11.3 Grants

**S1** (V only). At each opening, the period's S1 budget is `⌊balance · bounty_share / 1000⌋`. The
material lane reports each verified event:
- an exact-match case's first matching `Answer` in commit order, after the key's reveal:
  `s1_bounty` to the teacher;
- a `SyntheticProblem` or `HardCaseVariant` the head verifiably fails: `s1_setter_reward` to the
  setter.

Each is paid at once while the budget lasts; one that finds the budget spent is not paid. Nothing else
pays in S1.

**S2**, at a promotion (trust label T):
- `R = ⌊balance · promotion_share / 1000⌋`, taken from the balance;
- the trainer gets `⌊R · s2_trainer / 1000⌋`.

The data share is `D = R − trainer`, split over the registered datasets the winner declared, in its
declared order:
- each dataset gets `⌊D · w_d / 1000⌋` (`Σ w_d ≤ 1000` by the declaration's form);
- each dataset is capped at `⌊R · s2_dataset_cap / 1000⌋`;
- each dataset's registrant is capped, over all its datasets, at `⌊R · s2_contributor_cap / 1000⌋`.

What the caps withhold returns to the balance. Every S2 grant, and the winner's candidate bond, vests
(§17.11.4).

**Evaluation fees** are the executors' (§17.11.2); they are not grants.

### 17.11.4 Vesting and forfeit

A vesting grant is `{ recipient, stage, amount, label, vest_from_daa, vest_unit_daa = L_e,
vest_epochs, vested, forfeited }`, made at the epoch's decision with `vest_from_daa` = that DAA. Whenever
the line is advanced — and the line is due at every grant's next step (§17.5.2) — each grant's vested
target is `⌊amount · min(vest_epochs, ⌊(DAA − vest_from_daa) / vest_unit_daa⌋) / vest_epochs⌋`, and
the difference from `vested` is paid. The last unit pays the remainder.

**Vesting is final** [D12]: a vested amount is never confiscated. A forfeit (§17.10.3) first vests the
grant to the forfeit's DAA, then moves `amount − vested` to the balance, as forfeited, and marks the
grant.
Vesting counts DAA in units of the policy's `L_e`, not epochs that open, so a line that stops opening
epochs still vests [E8].

### 17.11.5 The earnings ledger, conservation and dissolution

Every sompi the pool pays — a refund, a grant's vested part, an S1 event, an executor's fee, the
dissolution balance — is first credited to its recipient bond in `improvement_earnings` (the pool
counts it as paid or refunded then). The step-2 flush turns earnings into pending payout rows: at most
`PALW_IMPROVE_PAYOUTS_PER_BLOCK_V1 = 2` a block, lowest bond key first, each paying the bond's whole
balance to its payout payload, and never while fewer than `PALW_IMPROVE_PAYOUT_QUEUE_RESERVE_V1 = 512`
rows of the queue's 1,024 are free. A bond that no longer exists has nobody to pay: its earnings are
burned. The payout's key is `H("…/payout/v1", borsh(bond) ‖ LE u64 DAA ‖ block)` with its first byte
forced to `0xFD`.

The pool keeps

```
deposited + fees_in + phi_in + held_in = balance + held + unvested + paid + refunded
```

`held` is every bond, candidate payment and escrow not yet refunded, forfeited or spent; a forfeit moves
held into the balance (and counts in `forfeited_in`, which the sum does not read).

Nothing is minted; nothing goes below zero; nothing vests past its amount.

When an opting-out line is past its effective DAA with no epoch open, no unvested grant and nothing
held: the balance goes to the spec 15 owner's earnings; every row of the line is deleted but its header,
which stays `Dissolved`.

### 17.11.6 The read door

A node reads the open epochs through `ConsensusApi::palw_improvement_open_epochs_v1`: per open epoch
(at most `max_open_epochs`), the line's header and policy, the epoch's header, its candidates in
acceptance order, and its items in item order (dropped ones included).

## 17.12 Rules

These refine RFC-0004's *Proposed Spec text*. MIP-1…MIP-20 keep its numbers; 21–24 are new.

- **PALW-MIP-1 (the method is free).** A candidate MUST NOT be required to have been produced by the
  VM, or by any particular method, tool or party.
- **PALW-MIP-2 (four checks).** The protocol MUST verify:
  - a candidate's IR class admission and family relation;
  - its declarations against the provenance policy (form and references);
  - its evaluation results;
  - its promotion eligibility.
  It MUST NOT condition anything else on how a candidate was produced.
- **PALW-MIP-3 (governed lines).** A line is governed only by its owner's policy object (§17.4.2). A
  policy MUST come into force only between epochs; an opt-out MUST take effect only after the current
  epoch and `grid` DAA (§17.4.4).
- **PALW-MIP-4 (the head).** A governed line's head MUST be an IR class id in its line row. It MUST move
  only by promotion or rollback. The protocol MUST NOT write spec 15's version rows or `current`. A
  developer's promotion MUST be refused on a governed line (§17.4.1).
- **PALW-MIP-5 (epochs).** A governed line MUST run at most one epoch at a time, advanced as §17.5 says.
- **PALW-MIP-6 (opening).** An epoch MUST open at the first grid boundary at which the head's usage since
  its restart reaches the threshold (§17.4.5, §17.5.3).
- **PALW-MIP-7 (the dataset root).** `dataset_root` MUST be fixed at `t_fix` over the material of
  §17.6.1, and never change.
- **PALW-MIP-8 (candidates).** Candidates MUST be entered in `[t_fix, t_close)`, at most `k_max`, each an
  admitted IR class of the parent's family, with its payment (§17.7, §17.11.1).
- **PALW-MIP-9 (the hold-out).** Items MUST be drawn as §17.8.1 says, from hold-out cases and committed
  setter sets. Prompts and teacher-forced references MUST be disclosed only after `t_close`, keys of
  generated items only in `Closing` with no claim on their items pending [E17]. An item missing a
  subject's evaluation MUST count for the parent. A setter that misses any key's reveal MUST lose its
  whole set and its bond [D15].
- **PALW-MIP-10 (evaluation jobs).** Every item MUST be evaluated for every subject by jobs whose
  contexts the chain derives, with one seed per item shared by every subject.
- **PALW-MIP-11 (scores).** A score MUST be the committed output of the policy's scoring stage, recorded
  once (§17.8.3).
- **PALW-MIP-12 (judges).** A judge MUST be drawn as §17.8.1 says, and excluded when it fails its
  anchors (§17.9.3). A judge MAY block a promotion and MUST NOT make one.
- **PALW-MIP-13 (promotion).** The fold MUST decide exactly as §17.9 says, with the pinned table.
- **PALW-MIP-14 (rollback).** Only the latest promotion MAY be rolled back: by the owner within its
  window, or by anyone with a proof of §17.10.1's valid kinds.
- **PALW-MIP-15 (composite artifacts).** A composite candidate:
  - its first `P` params MUST be the parent program's, in order, with the parent's declarations,
    served under the parent's artifact root;
  - its artifact root MUST bind the parent class, the parent's artifact root, the adapter section's
    root and `P`;
  - a candidate that changes any parent-side param MUST be a full-weight candidate;
  - every block MUST stay within 512 nodes.
- **PALW-MIP-16 (rewards).** Payouts MUST come only from the pool. S1 MUST pay only EXACT-verified
  events. S2 MUST pay only registered datasets the winner declared, within their caps. S2 grants and
  the winner's bond MUST vest, on their own steps, and their unvested remainder MUST be forfeited on a
  valid rollback; a vested amount MUST NOT be forfeited [D7, D12].
- **PALW-MIP-17 (not a reward path).** Nothing in this chapter MAY create a quantum, a ticket or
  eligibility, or draw on PALW worker rewards.
- **PALW-MIP-18 (licences).** A declaration of `LICENSED_DISTILL` MUST cite a registered, unexpired
  `TeacherLicence` that covers the use. A governed line's base licence class MUST be declared.
- **PALW-MIP-19 (privacy).** A hard case MAY use a job's prompt only under that job's `DataUseOptIn`, and
  its prompt MUST match the job's committed prompt hash.
- **PALW-MIP-20 (capacity).** Evaluation claims MUST reserve capacity as claims do (ADR-0160). An
  epoch's evaluation positions MUST NOT exceed the policy's `max_eval_positions`, nor the network's
  ceiling as a share of claim capacity (§17.8.4: each job is held to an equal share of the budget).
- **PALW-MIP-21 (conservation).** The pool MUST satisfy §17.11.5 after every block. Every sompi it pays
  MUST have been taken in by §17.11.1.
- **PALW-MIP-22 (abort).** A rollback while an epoch is open MUST abort it and refund every candidate its
  bond and unspent escrow in full, and its fee less its share of the evaluation spend the fees funded
  (§17.10.3) [E21]; the balance MUST NOT go below zero.
- **PALW-MIP-23 (the policy check).** A policy MUST satisfy every check of §17.4.3. `w_eval` MUST exceed
  `beacon_delay` by more than 32 DAA.
- **PALW-MIP-24 (order).** Transitions due in one block MUST be applied before that block's objects,
  lines in `(next_due_daa, line_id)` order, and each line's transitions in §17.5.3's order.

## 17.13 Vectors (the plan)

Vectors live under `consensus-vectors/improve-v1/`, as JSON with hex bytes. Two implementations —
the fold, and `misaka-palw-improve-ref2` — MUST agree on every one.

| File | What it pins | Owner |
| --- | --- | --- |
| `sign-table.json` | `sign_table_id`, the byte form's length, and 200 sample entries (every `m ≤ 20` at `K ∈ {1, 2, 4, 64}`, and edges at `m = 2048`) | core (A8) |
| `policy.json` | policy digests; one refusal per row of §17.4.3's table; the policy message | core (A2) |
| `material.json` | material frontiers and roots for 0, 1, 2, 3, 7, 8 and 33 leaves; `dataset_root` | core (A3) |
| `draw.json` | seeds, order keys, drawn items under caps, suite item ids, judge draws, pairwise order bits | core (A3) |
| `transitions.json` | scripted epochs as `(DAA, event) → state` traces, including DAA jumps across several times, `NoCandidate`, `TooFewItems`, an abort by rollback, and an opt-out | core (A3) |
| `promotion.json` | per-candidate score tables and their counts, eligibility bits and the decision, including the missing-evaluation rule, dropped items, anchors, both guards and ties | core (A8) |
| `pool.json` | a line's pool through deposits, φ, fees, escrow, S1, S2 caps, vesting, forfeit and dissolution, with the conservation sums | core (A9) |
| `scoring/*.json`, `pipelines/eval-*.json` | the scoring library (`consensus-vectors/tir-v2/`) | eval (A7, landed) |
| `composite.json` | composite roots and class ids | cand (A5) |
