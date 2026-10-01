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
>   `docs/design/palw/improve/ref2-findings.md`) are resolved here, each where its id is cited
>   ([S1]…[S5], [P1]…[P9], [E1]…[E15]).
> - The coordinator's decisions are applied: E11, E12, P1, the head's place (RFC §3 amended), and the
>   missing-evaluation rule.

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
| 9 | `improvement_pool_entries` | (line, epoch, index) → `PalwPoolEntryV2` | 8 | `0xD6` |
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
| Value bytes | `scoring_set_id` (64) ‖ `sign_table_id` (64) ‖ LE u16 `court_version` ‖ u8 `max_candidates_per_epoch` ‖ LE u32 `max_items_per_epoch` ‖ LE u64 `max_eval_positions_per_epoch` ‖ LE u16 `max_eval_budget_permille` ‖ LE u32 `max_governed_lines` ‖ LE u32 `max_policy_bytes` ‖ LE u32 `max_open_epochs` ‖ LE u32 `max_live_results` ‖ LE u16 `max_eval_seat_permille` (appended last, 2026-09-30) |
| Normaliser | `Some(never())` collapses to `None`; `for_each_fence` visits the activation only |
| Fork id | listed in `palw_fences_v1` as `("palw_improvement_v1", activation)` |
| Mirror | `PalwStateParamsV2::improve_from_daa`, `improve_ceilings` and `improve_lifecycle_daa` (borsh-skipped; the last is the ruleset's claim lifecycle bound, §17.4.3 rows 19–20), written by `Params::sync_palw_improvement_v1` only |
| Drill | `--palw-drill-improve-at` (`palw_drill_improve_fence_at_v1`, list `PALW_DRILL_IMPROVE_FENCES_V1`, ceilings `PALW_DRILL_IMPROVE_CEILINGS_V1`) |
| Format caps | candidates ≤ 16, items ≤ 4,096 per epoch, positions ≤ 2^40 per epoch, budget ≤ 1,000 ‰, lines ≤ 1,024, policy ≤ 16,384 bytes, open epochs ≤ 64, **live results ≤ 2^17**, **seat share ≤ 1,000 ‰** (zero allowed: no seat share); the others none zero. `max_live_results` is the product bound every block's root rehash pays: an epoch reserves `(n + regression_items + safety_items) × (k_max + 2)` result rows when it opens (the policy check refuses a policy whose epoch could never fit), and frees them when its results retire; an epoch opens only if the reservation fits. The drill's ceilings are (8, 1,024, 2^32, 500, 64, 16,384, 8, 2^14, 500) |
| Prerequisites | a `ConsensusV2` ruleset (checked first); `palw_audit_2026_09_11` declared; `palw_tir_v1`, **`palw_tir_fence2`**, `palw_gen_v1` and `palw_kary_court` in force at or below the activation (the second IR fence since 2026-09-29: every evaluation pipeline sized under H7, and no verdict may flip mid-epoch); **and `palw_fp_decode_rules` in force at or below the activation** **[decision A6-1, the coordinator's, 2026-09-30]** — an evaluation claim is an FP Job V4's bytes under the decode rules (§17.8.4), so a ruleset that armed improvement without them would open epochs whose claims the header-context door refuses, and every such epoch would end `NoChange`. `validate_palw_v2` refuses such a ruleset, as it refuses `palw_fp_job_v5` without them; the integration adds the check and a decode-rules entry (`--palw-drill-decode-rules-at`) to the flag-day and drill lists. |
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
| `misaka-palw/improve/suite-draw/v1` | the entries a suite draws from its dataset (§17.8.1) |
| `misaka-palw/improve/suite-leaf/v1`, `…/suite-node/v1` | a suite dataset's tree (§17.6.3) |
| `misaka-palw/improve/judge-template/v1` | a judge template dataset's one entry (§17.8.5) |
| `misaka-palw/improve/judge/v1` | the judge draw (§17.8.1). *`…/pair-order/v1` is retired by the 2026-09-30 decision: a Pairwise score shows both orders (§17.8.5)* |
| `misaka-palw/improve/eval-seed/v1` | an item's generation seed, `H(epoch seed ‖ LE u32 item)` |
| `misaka-palw/improve/eval-job/v1` | an evaluation job's id (A6: `H(line ‖ LE u64 epoch ‖ LE u32 item ‖ borsh(subject) ‖ kind ‖ part)`, with `borsh(subject)` = `0x00` (Parent), `0x01 ‖ class` (Candidate) or `0x02 ‖ class` (Previous), `kind` one byte (ExactMatch 1, RefLogLik 2, Judge 3, Pairwise 4 [S9]) and `part` one byte, 0 for every kind but a judged one, whose score is several claims, §17.8.5) |
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

**Prerequisite: `palw_fp_decode_rules`** [decision A6-1, 2026-09-30]. `palw_improvement_v1` REQUIRES
`Params::palw_fp_decode_rules` in force at or below its activation, and `validate_palw_v2` refuses a ruleset
that arms improvement without it. An evaluation claim is an FP Job V4's bytes under the decode rules (§17.8.4):
below them its header-context door refuses it by name, so a line governed on such a ruleset would open epochs
no claim could answer, and every one would end `NoChange`. The requirement is a check on the schedule, not a
second fence: nothing is armed by it, and a ruleset with `palw_improvement_v1` absent or dormant is untouched.
The integration (lane A) adds it with `palw_fp_decode_rules`'s own flag-day and drill entry
(`--palw-drill-decode-rules-at`), so the drill that arms improvement arms the rules first.

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
| `barred` | at most 16 submitters barred after a rollback, each with the DAA its bar ends |
| `last_promotion`, `regression_epoch`, `regression_check` | the epoch of the latest promotion and of its regression check (kept while a rollback can name them), and the predecessor the next epoch checks (§17.10.2) |

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
  - with no epoch open, the policy is in force at once;
  - otherwise it becomes `pending_policy`, replacing any earlier pending one, and is in force from
    the epoch's end [E6];
  - a pending opt-out is cancelled.
- **Opt out** (row, `policy = None`): §17.4.4.

### 17.4.3 The policy and its check

```
PalwImprovementPolicyV1 {
  version: u16 = 1,
  usage: { measure: Claims | WorkLeaves, value: u128 },
  windows: { grid, w_collect, w_submit, w_holdout, w_eval, beacon_delay, court_margin },
  eval: { stages: [ { kind, params } ], regression_dataset, regression_items, safety_dataset,
          safety_items, judge_set: [class], judge: Option<JudgeSpec>, pairwise: Option<JudgeSpec>,
          seat_pool_permille, anchor_floor_permille, n, n_min, delta_permille,
          epsilon_permille, epsilon_safety_permille, alpha_permille, max_new_tokens, stop_ids,
          setter_cap_permille, max_eval_positions },
  k_max: u8,
  fees: { registration_fee, candidate_bond, eval_fee_per_job, hard_case_fee, artifact_bond, setter_bond,
          dataset_bond, s1_bounty, s1_setter_reward },
  phi_permille, bounty_share_permille, promotion_share_permille,
  s2_trainer_permille, s2_dataset_cap_permille, s2_contributor_cap_permille,
  provenance: { teacher_classes: u8 mask, licence_classes: [id], full_weight_candidates: bool,
                base_licence_class },
  rollback_epochs, vest_epochs, ban_epochs: u32,
}
policy_digest = H("misaka-palw/improve/policy/v1", borsh(policy))
L_e = w_collect + w_submit + w_holdout + w_eval + court_margin        // the nominal epoch length
```

`PalwScoringParamsV1` is one of:
- `ExactMatch { open: i32, close: i32, key_cap: u32 }` (−1: no delimiter);
- `RefLogLik { logit_scale_q24: i32 }`;
- `Judge { lo: i32, hi: i32 }`;
- `Pairwise { margin: i32 }`.

The chain derives each stage's program from its kind, these parameters and the subject's shape, with
the builders `scoring_set_id` pins (A7).

`JudgeSpec { template_dataset: id, verdict_a: [u32], verdict_b: [u32], logit_scale_q24: i32 }` is what a judged
stage needs (§17.8.5): `Some` exactly when the stages hold the matching kind (`judge` for Judge, `pairwise` for
Pairwise). `template_dataset` names a registered dataset of exactly one entry, the stage's canonical prompt
template; the two verdict sequences are the token ids the judge scores ("Yes" and "No"); `logit_scale_q24` is the
judge's scale. `regression_dataset` and `safety_dataset` name the registered public datasets the suites draw from
(§17.6.3, §17.8.1): the zero id exactly when the suite has no items. `seat_pool_permille` is the share of an
evaluation job's fee its panel's credited seats divide (§17.11.2).

A policy is refused by name unless every check below holds. `C` is the fence's `ceilings`.

| # | Check |
| --- | --- |
| 1 | `version = 1`; `borsh(policy)` is at most `C.max_policy_bytes` |
| 2 | `usage.value ≥ 1` |
| 3 | every window `≥ 1` and `≤ 2^32`; `grid ≥ L_e` (so at most one epoch runs per grid period and every epoch ends before the next boundary it could open at) |
| 4 | **`w_eval > beacon_delay + PALW_IMPROVE_MIN_CLAIM_WINDOW_DAA_V1`**, where the constant is 32 DAA [E12]: the draw leaves at least 32 DAA of claiming |
| 5 | `1 ≤ \|stages\| ≤ 8`; each stage's params match its kind; at least one stage is primary (ExactMatch or RefLogLik); no kind appears twice |
| 6 | ExactMatch: `1 ≤ key_cap ≤ max_new_tokens`, `open ≥ −1`, `close ≥ −1`. RefLogLik: `logit_scale_q24 > 0`. Judge: `lo < hi`. Pairwise: `0 ≤ margin ≤ i32::MAX` (the stage's input interval) |
| 7 | a Judge or Pairwise stage requires `1 ≤ \|judge_set\| ≤ 16`, the set without duplicates, each an admitted IR class; without one, `judge_set` is empty; `anchor_floor_permille ≤ 1000` |
| 7b | **each judged stage carries its specification** [decision, 2026-09-30]: `judge` is `Some` exactly when there is a Judge stage and `pairwise` exactly when there is a Pairwise stage; each names a template dataset (non-zero id), two different verdict sequences of 1 to 8 ids each, and `logit_scale_q24 > 0` |
| 8 | `1 ≤ n_min ≤ n ≤ min(C.max_items_per_epoch, PALW_IMPROVE_SIGN_N_MAX_V1 = 2048)` |
| 9 | `regression_items, safety_items ≤ 1024`; **a suite with items names its dataset (a non-zero id), and a suite with none names none** — a policy whose suite names no dataset is refused [decision, 2026-09-30] |
| 10 | `delta_permille, epsilon_permille, epsilon_safety_permille ≤ 1000` |
| 11 | `alpha_permille` is one of the sign table's levels, `{10, 50}` [P1] |
| 12 | `1 ≤ max_new_tokens ≤ 4096`; `\|stop_ids\| ≤ 16`; `1 ≤ setter_cap_permille ≤ 1000` |
| 13 | `1 ≤ max_eval_positions ≤ C.max_eval_positions_per_epoch` |
| 14 | `1 ≤ k_max ≤ min(C.max_candidates_per_epoch, PALW_IMPROVE_SIGN_K_MAX_V1 = 64)` |
| 15 | `phi_permille, bounty_share_permille, promotion_share_permille, s2_trainer_permille ≤ 1000`; `1 ≤ s2_dataset_cap_permille, s2_contributor_cap_permille ≤ 1000` |
| 16 | `teacher_classes ≠ 0` and within the six defined bits; `\|licence_classes\| ≤ 32`, without duplicates; `base_licence_class ≠ 0` |
| 17 | `1 ≤ vest_epochs ≤ 64`; `rollback_epochs ≤ 16`; `ban_epochs ≤ 256` |
| 18 | `seat_pool_permille ≤ C.max_eval_seat_permille` (§17.11.2) |
| 19 | **`court_margin ≥ Λ`**, where `Λ` is the ruleset's unchallenged claim lifecycle (below) [decision, 2026-09-30, refined the same day]: a claim accepted in the epoch's last DAA before `t_eval` and left unchallenged could not otherwise reach `Final` before `t_score` |
| 20 | **a judged policy** (a Judge or Pairwise stage): `w_eval > beacon_delay + Λ + 32` — a judge reads FINAL generations, so the epoch evaluates in two rounds (generations, then judged parts), each needing the unchallenged lifecycle |

`Λ = anchor_delay + window_receipt + max(window_challenge, the short challenge window where scheduled)` — the
**unchallenged finalization path** under the ruleset's own windows: the anchor slot that binds the claim's panel,
the receipt window in which the panel licenses it, and the challenge window it finalizes after. **The court
window is not in it** [coordinator's refinement, 2026-09-30]: bounding the epoch by a dispute's whole course (≈ 4.8k
DAA with the 3,000-DAA court window) would make epochs days long and refuse every practical policy, and a
claim that is not `Final` at the decision point is simply *missing* — the missing-evaluation rule (§17.9.1)
gives its item to the incumbent. A court that convicts such a claim later still slashes it (the claim rules,
independently of the epoch's decision); a conviction before the decision also voids the claim, which frees its
job and keeps its score out of the sign test (§17.8.6). `Λ` is computed from the ruleset
(`palw_improvement_claim_lifecycle_v1`) and mirrored in the state parameters (`improve_lifecycle_daa`). Rows 19
and 20 are asked at policy registration where the mirror is set (every ruleset that arms the fence); a policy
they refuse is unworkable on this ruleset and is refused by name. *The "submission" window the coordinator's
decision lists is read as the window evaluation claims are submitted in, `w_eval − beacon_delay` — the one
window of the epoch in which a claim is submitted; `w_submit` holds no claim and is not held to `Λ`.*

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

A policy object with `policy = Some` before `effective_daa` cancels the opt-out.

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
line_id)` order. It advances each one by applying every transition due at this DAA, in the order of
§17.5.3, until none is due [E15]. It then vests the line's grants (§17.11.4) and sets `next_due_daa` to
the smallest DAA at which a transition, a vesting step or the opt-out becomes due:
- for an idle line: the next grid boundary strictly after this DAA;
- for an open epoch: its next time.

A block whose DAA jumps past several times therefore applies all of them, in order, in that block.

### 17.5.3 Transitions

Let `W` be the policy's windows. All times are fixed when the epoch opens.

1. **Opening.** At a grid boundary — the first block with `DAA ≥ g` for a multiple `g` of `grid`,
   taking `g` as the largest such multiple ≤ DAA — an idle, governed line with `usage ≥ usage.value`
   opens epoch `e = next_epoch`:
   - `t_open = g`, `t_fix = g + w_collect`, `t_close = t_fix + w_submit`, `t_draw = t_close + w_holdout`,
     `t_eval = t_draw + w_eval`, `t_score = t_eval + court_margin`;
   - `parent = head`, and `previous` as §17.10.2 says;
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
   and the judges. Then the parent's evaluation escrow is reserved from the pool (§17.11.2).
   - With fewer than `n_min` drawn items: **decided at once** `NoChange(TooFewItems)` [E9]. (Which
     items are primary is known only from their scores; scoring applies the same test to the primary
     count, §17.9.5.)
   - With a pool too small for the parent's escrow: `NoChange(PoolInsufficient)`.
6. **`Evaluating → Closing` at `t_eval`.** No evaluation claim is accepted from here on.
7. **Scoring**, at the first DAA ≥ `t_eval` at which the evaluation lane reports every evaluation claim
   of the epoch final and the material lane reports no drawn set or case owing a reveal, and at the
   latest at `t_score` [E4]. Just before it, the material lane drops every drawn item still owing a
   reveal (§17.9.1). The
   fold computes the counts and the decision (§17.9), and applies it (§17.9.5, §17.11). The epoch is
   `Decided`.
8. **The epoch's end** (whenever an epoch becomes `Decided`, whatever the path):
   - `open_epoch = None`, `decided_daa = DAA`;
   - a pending policy comes into force;
   - a pending opt-out starts its delay;
   - unused escrows are returned (§17.11.2);
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
  items }`, at most 16 sets. Its items are `(set_id, i)` for `i < items`. In v1 setter items are
  generated only: an ExactMatch key, or an empty key for a judged item. Likelihood items come from
  hold-out cases with a continuation reference, disclosed from the draw by `HardCaseKeyRevealed` —
  one hash commits a set's keys, so a set cannot disclose a reference at the draw apart from its keys
  (PALW-MIP-9).

### 17.6.3 Suites: registered public datasets [decision, 2026-09-30]

The regression and safety suites are **guards, never a gain measure** (§17.9.3: they enter the sign test only as
a no-regression bound). Their items come from **registered, public datasets** (`DatasetRegistered`, tag 76) the
policy names — never from a bare root — and are drawn from them by R (§17.8.1); their prompts are disclosed
through the dataset's own committed content (§17.8.4).

- **The content.** A suite dataset's `content_root` is the root of an RFC 6962 Merkle tree over its `items`
  entries: a leaf is `H("misaka-palw/improve/suite-leaf/v1", borsh(entry))`, a node is
  `H("…/suite-node/v1", left ‖ right)`, and the tree of `n` leaves splits at the largest power of two below
  `n` (one leaf is its own root). An entry is `PalwSuiteEntryV1 { prompt: [u32], reference }` with
  `reference = ExactKey([u32]) | Continuation([u32])`: an exact-match key (the answer span a generation must
  equal), or a likelihood reference (the continuation a teacher-forced pass scores).
- **Registration is not verification.** The chain cannot see the content, so `DatasetRegistered` binds only the
  root and the count; a dataset whose root is not such a tree has entries nobody can open, and each of its
  suite items counts for the incumbent (§17.9.1). That fails closed: only the line's own candidates lose.
- **What a policy needs.** A suite with items names a dataset registered on this line with at least that many
  entries (and at most 2^32), checked when the policy object is applied (§17.4.2); a policy whose suite names no
  dataset is refused by §17.4.3 row 9 (the pure check), and one whose dataset is missing or too small by the
  fold. A template dataset (§17.8.5) is a registered dataset of exactly one entry.
- **Public means public.** A suite's prompts and keys are readable by everyone, a candidate's trainer included.
  That is why a suite never measures gain: memorising it earns nothing, and failing it blocks.

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
- registration fees stay in the pool (except on abort, §17.10.3).

## 17.8 Evaluation

### 17.8.1 The draw [E7], [E10]

At the block that moves the epoch to `Evaluating` (the first with `DAA ≥ t_draw + beacon_delay`):

```
seed        = H("misaka-palw/improve/epoch-seed/v1", block_hash ‖ line_id ‖ LE u64 epoch)
entry ids   : a hold-out case         → case_id
              setter item (s, i)       → H("…/setter-item/v1", set_id ‖ LE u32 i)
order key   = H("misaka-palw/improve/draw/v1", seed ‖ entry id)
```

`block_hash` is the hash of the chain block whose fold performs the draw. It postdates the hold-out, so
the draw is fixed only after every candidate and every hold-out case is.

- **Draw.** The pool is sorted by `order key` (as bytes, ascending; ties are impossible). The items are
  the first entries of that order, with two exclusions:
  - an entry whose supplier (the case's submitter or the set's setter) already supplies
    `cap = max(1, ⌊κ·n / 1000⌋)` drawn items [κ = `setter_cap_permille`] is skipped;
  - the draw stops at `n` items.

  Items are without replacement. A pool smaller than `n` gives all of it [E10]. Drawn items are
  numbered `0..` in draw order.
- **Suites** [decision, 2026-09-30]. Then, for each suite with items, `min(items, entries)` distinct entries of
  its registered dataset (§17.6.3) are drawn by a sparse Fisher–Yates shuffle driven by the epoch seed — for
  `j = 0, 1, …`: `r_j = LE u64(H("…/suite-draw/v1", seed ‖ dataset_id ‖ role ‖ LE u32 j)[0..8]) mod (n − j)`
  over the `n` entries, the entry at virtual position `j + r_j` is drawn and the entry at `j` takes its place
  (`role` 1 for the regression suite, 2 for the safety suite, so two suites over one dataset draw different
  entries) — at a cost of `O(items · log items)`, whatever `n` is. Each drawn entry `e` is appended as an item
  whose id is `H("…/suite-item/v1", dataset_id ‖ LE u32 e)` and whose source is `Regression { index: e }` or
  `Safety { index: e }`. Suites are evaluated whole, every epoch [P4], and do not count toward `n`. A dataset
  smaller than the suite gives all of it.
- **Per item.** `seed_i = H("…/eval-seed/v1", seed ‖ LE u32 i)`: one per item, the same for every
  subject.
- **Judges.** When the policy has a Judge or Pairwise stage, a judge is drawn for each drawn (non-suite)
  item: `judge_set[LE u64(H("…/judge/v1", seed ‖ LE u32 i)[0..8]) mod |judge_set|]`. Suite items are never
  judged.
- **Pairwise order — retired** [decision, 2026-09-30]. A Pairwise score shows the judge BOTH orders of the pair
  (§17.8.5), so no order is drawn; the `pair-order` key is unused.
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
- job ids `H("…/eval-job/v1", line ‖ LE u64 epoch ‖ LE u32 item ‖ borsh(subject) ‖ kind)`;
- open claiming in `[draw, t_eval)`;
- the fee per job, paid from the subject's escrow;
- capacity reservation (§17.8.4).

**Key disclosure** (RFC §7.1, amended):
- setter prompts, and the references of teacher-forced items, are disclosed at the draw;
- keys of generated items are disclosed only after every subject's generation claims on the set's
  items are final;
- a steward that misses its reveal forfeits its bond, and its items drop out for every subject.

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
| Judge | the judge's margin `LL(verdict_a) − LL(verdict_b)` on the subject's output, in Q24 nats, clamped to the stage's `[lo, hi]` (§17.8.5) |
| Pairwise | the outcome from the candidate's side: **+1** when the sum of the judge's two margins (both orders shown, §17.8.5) exceeds the stage's `margin`, **−1** otherwise — a tie goes to the incumbent, so it is never 0 |

A second score for the same `(item, subject, kind)` is refused. A score is taken only while the epoch
is `Evaluating` or `Closing`, for an item it drew and a subject it evaluates.

**ExactMatch is scored by the fold at the key's reveal**, not by a claim: keys stay hidden until every
subject's outputs are final, so no single claim may read one. An ExactMatch job is generation only; its
claim carries the generated ids, and the evaluation lane keeps the answer span's hash in its job row.
The arm that reveals a key (A4: `SetterKeysRevealed`, or a case's key) computes each subject's
pass/fail with the evaluation lane's `palw_improve_exact_match_score_v1` and records it here. **A suite entry's
key is public** (§17.6.3), so its generation is scored from the opened entry at acceptance and recorded at its
`Final`, with no reveal. RefLogLik scores are recorded at `Final` from the claim's committed score lanes; **a
Judge or Pairwise score is recorded when the last of its parts is final** (§17.8.5). The policy's `key_cap`
bounds a key's length, a suite's included.

### 17.8.4 The evaluation claim (A6)

An evaluation job is claimed by a **free-prompt commitment** (subnetwork `0x4a`) whose job is **version 9**
[the coordinator's decision of 2026-09-29]; nothing new is registered and no object tag is taken. The rules
below are the evaluation lane's (`palw_improve_eval_v1`, `palw_improve_eval_fold_v1`).

**Carriage.**
- The job is an FP Job V4's bytes with the version word 9, then a `PalwEvalJobV1 { line_id, epoch, item,
  subject, kind, part, mode }` (the job's `tail`). `part` is 0 for every kind but a judged one, whose score is
  several claims (§17.8.5). Its fixed FP fields: public (`PublicDa`), its prompt on the
  payload (user mode), greedy (temperature 0, a zero seed), a zero nonce, canonical decode rules equal to the
  ones its mode derives (a generating job's stops and budget; a teacher-forced job's — and a judged
  part's — no-op rules), at most 4,096 ids in its stream, its kind's mode, and the class it runs: its
  subject's, or for a judged part (`mode = Judged { judge }`) the judge class.
- The payload is the FP payload's borsh, then `PalwEvalClaimTailV1 { generated: [u32], score: [i32],
  subject_layout, params, read: Option<PalwEvalReadV1>, opening: Option<PalwSuiteOpeningV1> }`. `read` is a
  judged part's — the template it filled (opened) and the lengths dividing its prompt into the item's prompt and
  the outputs it read, `{ template, item_len, output_lens }` — and exactly a judged part carries one;
  `opening` is a suite item's — `{ reference, proof }`, the entry's reference and its RFC 6962 inclusion
  proof (§17.6.3), the entry's prompt being the claim's own — and exactly a claim on a suite item carries one
  (the fold knows which). An FP decoder refuses the whole as undecodable, so a build below the fence
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
  ruleset's caps) under the ruleset's decode-rules door, then the claim's own stateless rules: the job's
  shape (a judged part runs its judge class, a part is in its kind's range), the tail against the roots, a
  judged part's prompt against its declared reading (the prompt is exactly the template's fill of the item's
  prompt and the outputs the lengths slice from it, whose roots are the claim's `finalized` root), and a suite
  opening's agreement with the job's kind and the claim's own prompt and ids.
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
- a class that is not the one the claim runs — its subject's, or its item's drawn judge for a judged part; a
  kind the policy does not score; a part past the kind's claims;
- a generating job whose seed, budget or stops are not the item's seed and the policy's;
- **a likelihood item** (a hold-out case whose reference is a continuation, or a suite entry whose reference is
  one) **taking a generation job** — one job per stage that applies to the item's kind (§17.8.2); a
  teacher-forced job whose reference is not the case's disclosed one, or not yet disclosed; a prompt that is not
  the item's disclosed prompt, or not disclosed;
- **a suite item** [decision, 2026-09-30] whose claim does not open its entry, whose opening does not verify under
  its dataset's registered `content_root` (the tree size is the dataset's `items`, never the claim's word), whose
  entry's reference is not the job's kind, or whose key is longer than the policy's `key_cap`; and **an opening
  on any other item**;
- **a judged part** (§17.8.5): a kind the policy has no judge specification for, an item whose drawn judge is not
  the claim's, a suite item, ids that are not the part's verdict sequence, a template that is not the
  registered dataset's one entry, a generation it reads that is not FINAL (the subject's, and for a Pairwise
  part the parent's too), outputs whose roots are not those final claims' `output_root` in the order the part's
  order shows, an item prompt that is not the item's disclosed prompt;
- stage parameters that are not the policy's (a judged stage's include its specification's scale); roots that
  are not the tail's;
- **the epoch's evaluation budget** (below); a class that is not an IR class; a layout that is not the one the
  class id binds; a `work_leaves` that is not the chain's closed-form count of the derived context;
- **a class the chain does not serve** (below), and **a class whose replay room has no more for evaluation**
  (below);
- the job already taken by a live claim (open claiming, below); the executor's exposure ceiling
  (`FreePromptExposureCeiling`), which the swing lock raises (below); the evaluation's share of the executor's
  room (below).

**Open claiming.** The first claim the chain holds takes the job. A claim voided or retired before `Final`
frees it — the row is cleared when the next claim for it is folded — so a dead claim never holds a job past
its own life.

**Capacity** (PALW-MIP-20; RFC-0004 §13, open question 7). Five bounds, all dormant below the fence:
- *Reservation, and the swing lock* [decision, 2026-09-30]. The claim is an FP claim with no quanta, no `pwu`,
  no receipt rights, no weight and no reward escrow (PALW-MIP-17). It reserves what a claim of that work on
  its class reserves — its step leaves at the class's `slash_value_per_pwu`, under ADR-0160's stage-1 rule
  `⌈w / ρ⌉` — **and at least what it could swing**, and passes the free-prompt arm's exposure ceiling, so a
  false claim is slashed like any other (a conviction takes `claim.reserved`). *Fraud gain is not zero*: a
  forged evaluation can flip a sign test, and a promotion pays `R = ⌊balance · promotion_share / 1000⌋`
  (§17.11.3) to the parties it favours. The collateral bar (the most a claim's executor can steal is at most what
  its lock recovers) sizes the lock per claim:

  ```
  swing(claim)  = ⌈ R / F ⌉          R = ⌊pool.balance · promotion_share_permille / 1000⌋  at the claim's acceptance
  reserved      = max( work lock,  swing )
  ```

  `F` is the fewest forged pair outcomes that flip the sign test. v1's **conservative** reading is `F = 1`
  (`PALW_IMPROVE_SWING_FORGED_PAIRS_V1`): an honest result one pair short of the critical value is flipped by
  one forged claim, and the attacker chooses its candidate — so `F` colluding claims' locks together cover the
  payout. Calibrating `F` (and `R`'s horizon: a pool can grow after acceptance) is D-M1's. The lock raises the
  claim's exposure like any reservation (an executor whose room cannot carry it is refused
  `FreePromptExposureCeiling`); the *share of the executor's room* bound below reads the work's own lock, which
  is capacity, not collateral.
- *The epoch's evaluation budget in positions* **[decision A6-2]**. A job's positions are its prompt's ids plus
  its stream's. The budget is `B = min(policy.max_eval_positions, C.max_eval_positions_per_epoch ·
  C.max_eval_budget_permille / 1000)`, and the epoch's claimable jobs are at most `J = (n·(1 + 2·[Judge]) +
  m)·S + 4·n·[Pairwise]·(S − 1)` over its `S` subjects, `m = regression_items + safety_items`: one job per drawn
  item and subject for the item's primary kind (an item is exact-key or likelihood, never both), two more for a
  Judge stage (a judged score is two claims, §17.8.5), four per item and non-parent subject for a Pairwise stage,
  and one per suite entry and subject (a suite item discloses its entry in its claim, §17.8.1). An ExactMatch
  key's scoring is the fold's, not a claim, so it adds none. A claim whose positions exceed `⌊B / J⌋` is
  refused. The shares are equal and independent, so no order of
  claims can spend another job's share, a voided claim returns nothing to anyone, and the epoch's total never
  exceeds `B`. *No chapter defines a claim capacity in positions; v1 takes the network's per-epoch ceiling as
  the span's capacity and `max_eval_budget_permille` as the evaluation's share of it — a network that wants
  the budget tighter lowers either ceiling. The RFC leaves the share open; this reading is the one the chain
  can compute.*
- *The share of the executor's room* **[decision A6-3]**. One evaluation claim may reserve at most
  `C.max_eval_budget_permille` of its executor bond's exposure ceiling (the room the bond carries; the work's own
  lock, not the swing), so an evaluation claim cannot take the room an executor's attempts need.
- *The class's replay room* **[decision A6-4, the coordinator's, 2026-09-30]**. An evaluation claim is **counted in
  its class's replay room (ADR-0137)** as one whole claim of the class, from acceptance until its licence (or
  `Final`, for a class held to Final), as an attempt is: a lane of the in-flight tally of its own
  (`eval_claims`, `licensed_eval_claims`; no quanta, so nothing to round into a canonical job), pooled with
  the class's attempts and free-prompt claims in the rate rule, in the registry's in-flight count, and in
  `check_class_admits_claim` (asked as the free-prompt arm asks it, past `palw_audit_2026_09_23`, with the
  claim itself as one more); the executor's share of the class's unlicensed claims (T-2(a)) is asked too. And
  **`max_eval_budget_permille` bounds them**: the class's evaluation claims whose replay is still owed, with
  the one asked about, may not exceed that share of the class's capacity in claims (at least one is always
  admitted) — evaluation cannot crowd out the class's attempts. A claim past either is refused by name.

**Panels** (who seats an evaluation claim, and who is paid). An evaluation claim is an FP claim of the class it
runs, so the chain derives its panel as it derives any claim's (the same seed, the same draw over the seats ready
for that class; no evaluation-specific code). The fold therefore requires the class to be one the chain serves:
not frozen (`FrozenClass`), admitted by the model registry where the registry governs (`ClassNotAdmitting`), and
verifiable inside the windows (`check_class_verify_admits_v1`, as a free-prompt claim asks it: an NM class, an
unmeasured deadline and a measured `D` over the cap are refused). Every seat that may verify a candidate must
hold its artifact; that is the registry's lifecycle for the candidate's class (Prefetching → admitting), which an
evaluation claim waits for. **The seats are paid from the epoch's evaluation escrow** [decision A6-4]: at `Final`,
`seat_pool_permille` of the job's fee is the seats' pool, divided over the seats DRAWN and paid to those credited
(the ordinary per-claim rule, `palw_panel_split_permille_v1`), the executor taking the rest (§17.11.2).

**Fees and scores at `Final`** (the claim's `finalize_claim`): the job's row records its finality; a kind that
committed a score records it through `record_improvement_score_v1` (RefLogLik at once; a Judge or Pairwise score
when the last of its parts is final, §17.8.5; an ExactMatch suite entry's pass/fail from its opened key); the
subject's escrow pays the job's `eval_fee_per_job` as earnings credits (§17.11.5), once — the executor's, and the
credited seats' share (§17.11.2). An ExactMatch generation of a hidden key is scored at its key's reveal
(§17.8.3). A claim final after the epoch stops taking scores earns neither: the escrow is the epoch's, and
returns with it.

**Late claims** [coordinator, 2026-09-30]. Claims are accepted before `t_eval` (open claiming), but one counts only
if it is `Final` when the epoch scores — in the first block with `DAA ≥ t_score = t_eval + court_margin`, or
earlier in `Closing` when nothing of the evaluation is still to come (§17.5.3 step 7). At that block every job
without a `Final` claim is **missing**, and §17.9.1's rule gives it to the incumbent: a missing candidate score
is a loss and a missing parent score a parent win, so a late claim can hurt the candidate it was meant to
evaluate and can never make a promotion. The pieces of the lane are consistent with it:
- *scores*: a claim that finalizes after the decision records none (the epoch no longer takes scores); an
  ExactMatch generation scored at a key's reveal needs a `Final` claim, and a missing one scores nothing;
- *fees and refunds*: such a claim is paid nothing — the subject's escrow is the epoch's, and what it did not
  spend returns at the epoch's end (§17.11.2) — and its executor bears the work; its reservation is released as
  any claim's is. Its job row retires with the epoch (§17.5.4), so a late `Final` finds no row and does nothing;
- *the sign test*: counts are over recorded scores only (§17.9.3), so a late or voided claim is the same as an
  unclaimed job: nothing recorded, the item for the incumbent;
- *courts*: a court that convicts a claim slashes it by the claim rules (`claim.reserved`, §17.8.4 above, the
  swing lock) independently of the epoch's decision, whenever it rules; before the decision it also voids the claim,
  which frees the job while the epoch still takes claims and keeps the score out of the sign test (§17.8.6).
The policy's windows hold the *unchallenged* path (§17.4.3 rows 19–20): an honest claim accepted before `t_eval`
finalizes by `t_score`; a challenged one may not, and is then a missing evaluation by the rule above.

### 17.8.5 Judged scores (A6) [decision, 2026-09-30]

A Judge or Pairwise score involves **no generation and no sampling**. It reuses the deterministic teacher-forced
RefLogLik pass (§17.8.3): the judge — a registered IR text class, drawn per item from the policy's `judge_set`
(§17.8.1), never the line's head nor a former head nor a candidate of the line (checked when the policy is applied
and when a candidate is admitted) — is run teacher-forced over a canonical prompt and scores one of two fixed
verdict sequences. The stage's `JudgeSpec` (§17.4.3) declares the template's registered dataset, the two
verdicts (`verdict_a`, `verdict_b`, 1–8 ids each: "Yes"/"No" for a Judge stage; for Pairwise, the first-shown
response is better / the second-shown is) and the judge's logit scale.

- **The template.** A registered dataset of exactly one entry, a `PalwJudgeTemplateV1 { segments: [[u32]] }` of
  three (Judge) or four (Pairwise) segments within 1,024 ids in all, whose `content_root` is
  `H("misaka-palw/improve/judge-template/v1", borsh(template))`. A part's prompt is the template filled:
  `seg₀ ‖ item prompt ‖ seg₁ ‖ output₀ ‖ seg₂ [‖ output₁ ‖ seg₃]`.
- **The parts.** A Judge score is two parts (`part` 0: `verdict_a`, 1: `verdict_b`), each a teacher-forced pass
  of the judge class over the template filled with the item's prompt and the subject's FINAL generation, the
  verdict as its reference; a Pairwise score is four (`part = 2·order + verdict`): both orders — order 0 shows
  the subject's generation first and the parent's second, order 1 the reverse — each with both verdicts. A part
  is an evaluation claim of its own (job key `(line, epoch, item, subject, kind, part)`), claimed openly, paid
  its fee at its `Final`, and scored `LL = hi·2^31 + lo` of the pass (the RefLogLik lanes). The fold reads the
  generations from its job table: the subject's ExactMatch job (and the parent's) must hold a FINAL claim, and the
  outputs the part's prompt carries (sliced from it by the declared lengths) must hash to those claims'
  `output_root`s, in the order the part's order shows. So an epoch with a judged policy evaluates in two
  rounds — generations, then judged parts — which the policy's windows must hold (§17.4.3 row 20).
- **The score** (when all the parts are final): **Judge** = `clamp(LL(verdict_a) − LL(verdict_b), lo, hi)`, the
  judge's preference for the first verdict on the subject's output. **Pairwise**: with `m₀ = LL(o0,a) −
  LL(o0,b)` (the preference for the first-shown, the subject's) and `m₁ = LL(o1,a) − LL(o1,b)` (the first-shown
  there is the parent's), the score is the sum of the two margins from the subject's side, `m₀ − m₁`, and the
  recorded outcome is **+1 when it exceeds the stage's `margin` and −1 otherwise — ties go to the incumbent**.
  A judge that always prefers the first-shown (or the second) has `m₀ = m₁` and scores 0: the sum cancels the
  position bias, and the incumbent keeps the item.
- **Missing parts.** A judged score with a part unclaimed or not final is missing: it counts for the incumbent
  (§17.9.1). A claim's reading needs final generations, so an item with no final generation of the subject
  (or, for Pairwise, the parent) has no judged score.

### 17.8.6 The evaluation court

*Reserved for the evaluation court (proofs 13–15, §17.0): added with its implementation.*

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
- The **primary** kind of an item is read from the recorded scores: ExactMatch if any subject has an
  ExactMatch score on it, else RefLogLik if any has a RefLogLik score; an item with neither but with a
  guard score is judged-only; an item with no score at all is a primary item every subject misses, so it
  counts, and counts for the parent (a Loss for every candidate). Regression and safety items without a
  score count the same way.

The evaluation lane's `palw_improve_item_outcome_v1` implements exactly this.

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
| `judge` | drawn items with a Judge score, their judge not excluded | |
| `pairwise` | drawn items with a Pairwise score, their judge not excluded | |

**Judge anchors [S4].** A judge is *excluded* for the epoch (its Judge and Pairwise scores void) when it
fails its anchors. The anchor pairs are, over the drawn exact-match items it judged, every ordered pair
of subjects `(p, f)` where `p` passed and `f` failed. A pair is *correct* when the judge's Judge score
of `p` exceeds its score of `f`. The judge is excluded if:
- it has fewer than `PALW_IMPROVE_MIN_ANCHOR_PAIRS_V1 = 8` pairs; or
- `1000·correct < anchor_floor_permille · pairs`.

A policy without a Judge stage has no anchors, and its Pairwise judges are never excluded.

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
`epoch`, with `previous = to_class`. A rollback of an older promotion is refused: forfeiting its unvested
rewards needs no rollback, and its class is no longer the head.

| Cause | Valid when | Head entry cause |
| --- | --- | --- |
| `Owner` | the filer is the spec 15 owner, and `DAA ≤ promotion.daa + rollback_epochs·L_e` [E8] | `RolledBackByOwner` |
| `LaterRegression { epoch: e2 }` | `e2 > epoch`, `e2` is decided, and its regression check shows `Previous` eligible against the parent with `K = 1` (§17.10.2) | `RolledBackByProof` |
| `CanaryFailed { claim, item }` | the evaluation lane records a canary job over `claim`'s final output that fails safety item `item`. **Not in v1 until A6 provides canary jobs**: refused by name | `RolledBackByProof` |
| `LicenceViolation { challenge }` | an upheld licence challenge. **Not in v1** (RFC open questions 10–11 give no challenge procedure): refused by name | `RolledBackByProof` |

### 17.10.2 The regression check

The first epoch that opens after a promotion evaluates the head's predecessor as the subject
`Previous(previous class)`. That requires:
- the head's last entry is `Promoted`;
- the predecessor is still an admitted class.

`Previous` runs the same jobs as a candidate, paid from the pool like the parent. Its counts
(`previous_counts`) are computed as a candidate's with `K = 1`. It is never a candidate: it cannot
win, and it does not count in `K`.

### 17.10.3 The effect

A valid rollback:
- sets `head = to_class`, with a history entry, and restarts `usage`;
- forfeits the rolled-back winner's unvested grants and its unvested candidate bond to the pool
  [PALW-MIP-16];
- bars its submitter until `DAA + ban_epochs·L_e`;
- **aborts an open epoch** [E11]: the epoch is `NoChange(Aborted)`, every candidate's submission fee,
  bond and unused escrow is refunded in full, and the evaluation spend already paid from the pool
  stays spent. No re-basing and no waiting: the next epoch opens from the restored head under the
  usual trigger.

## 17.11 The pool and rewards

### 17.11.1 Money in

Three paths bring money into the pool.

1. **Bond debits.** An object signed by a bond that owes a fee or a bond amount debits the signer's
   collateral by that amount, as a slash does. The debited sompi are destroyed from the collateral.
   The pool records what it owes: a fee as balance, a bond as held until refunded or forfeited, an
   escrow as held for the jobs it pays. The debit is refused by name unless the bond is `Active` and
   its free collateral covers it. Free collateral is its collateral less slashed, less what the
   R-core+ ledger holds committed, less its accuser ledger.
   - `CandidateSubmitted`: `registration_fee` (balance), `candidate_bond` (held), and
     `eval_fee_per_job × J` (the candidate's escrow). `J` is the most jobs the candidate can be given:
     `(n + regression_items + safety_items) × |primary and judge stages|`, plus `n` if there is a
     Pairwise stage.
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
`NoChange(PoolInsufficient)`. `J` bounds the jobs one subject can be given: two per item for its primary kind (a
generation, then its scoring once the key is out — the upper bound), **two per drawn item for a Judge stage and
four for a Pairwise stage (§17.8.5: a judged score is that many claims)**, and two per suite item.

The evaluation lane pays, when a job's claim is final, `eval_fee_per_job` from the subject's escrow, as earnings
credits (§17.11.5): **the executor's, and the credited seats' share of it** [decision A6-4, 2026-09-30]. With
`P = ⌊fee · seat_pool_permille / 1000⌋` (the policy's `seat_pool_permille`, at most the fence's
`max_eval_seat_permille`, §17.4.3 row 18), the seats' pool, and `d` the seats DRAWN for the claim's panel (its
duty row), each CREDITED seat — one that answered — takes `⌊P / d⌋` and the executor takes `fee − P`: the
ordinary per-claim rule (`palw_panel_split_permille_v1`). What no seat was credited for, and the division's dust,
is **not spent**: it stays in the escrow and returns with it at the epoch's end, and the executor never takes
it. *(The ordinary rule sends that remainder to the panel reserve; this escrow's accounting has no place for it,
and returning it is the conservative choice — an executor can never profit from a seat's silence.)* A claim with
no duty row (bound below the panel economy) pays its executor the whole fee. The escrow's `spent` grows by what
was paid, so a job whose escrow is exhausted pays only what is left.

At the epoch's end, whatever is left of an escrow goes back to where it came from:
- a candidate's, to its submitter;
- the parent's and `Previous`'s, to the balance.

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
vest_epochs, vested, forfeited }`. Whenever the line is advanced, each grant's vested target is
`⌊amount · min(vest_epochs, ⌊(DAA − vest_from_daa) / vest_unit_daa⌋) / vest_epochs⌋`, and the
difference from `vested` is paid. The last unit pays the remainder.

A forfeit (§17.10.3) moves `amount − vested` to the balance, as forfeited, and marks the grant.
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
  generated items only after every subject's outputs are final. An item missing a subject's evaluation
  MUST count for the parent.
- **PALW-MIP-10 (evaluation jobs).** Every item MUST be evaluated for every subject by jobs whose
  contexts the chain derives, with one seed per item shared by every subject.
- **PALW-MIP-11 (scores).** A score MUST be the committed output of the policy's scoring stage, recorded
  once (§17.8.3).
- **PALW-MIP-12 (judges).** A judge MUST be drawn as §17.8.1 says, and excluded when it fails its
  anchors (§17.9.3). A judge MAY block a promotion and MUST NOT make one. A judge MUST be run teacher-forced
  over the registered template (§17.8.5): no generation, no sampling; and MUST NOT be the line's head, a former
  head or a candidate of the line.
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
  the winner's bond MUST vest, and MUST be forfeited on a valid rollback.
- **PALW-MIP-17 (not a reward path).** Nothing in this chapter MAY create a quantum, a ticket or
  eligibility, or draw on PALW worker rewards.
- **PALW-MIP-18 (licences).** A declaration of `LICENSED_DISTILL` MUST cite a registered, unexpired
  `TeacherLicence` that covers the use. A governed line's base licence class MUST be declared.
- **PALW-MIP-19 (privacy).** A hard case MAY use a job's prompt only under that job's `DataUseOptIn`, and
  its prompt MUST match the job's committed prompt hash.
- **PALW-MIP-20 (capacity).** Evaluation claims MUST reserve capacity as claims do (ADR-0160), and lock at
  least what they could swing (§17.8.4). An epoch's evaluation positions MUST NOT exceed the policy's
  `max_eval_positions`, nor the network's ceiling as a share of claim capacity (§17.8.4: each job is held to an
  equal share of the budget). An evaluation claim MUST count in its class's replay room and MUST NOT take more
  than `max_eval_budget_permille` of it.
- **PALW-MIP-21 (conservation).** The pool MUST satisfy §17.11.5 after every block. Every sompi it pays
  MUST have been taken in by §17.11.1.
- **PALW-MIP-22 (abort).** A rollback while an epoch is open MUST abort it and refund every candidate in
  full (§17.10.3).
- **PALW-MIP-23 (the policy check).** A policy MUST satisfy every check of §17.4.3. `w_eval` MUST exceed
  `beacon_delay` by more than 32 DAA, and the windows MUST hold the ruleset's claim lifecycle `Λ` (rows 19–20).
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
| `draw.json` | seeds, order keys, drawn items under caps, **the suite draw (entries of a registered dataset by the epoch seed)**, suite item ids, judge draws | core (A3); the suite draw 2026-09-30 |
| `transitions.json` | scripted epochs as `(DAA, event) → state` traces, including DAA jumps across several times, `NoCandidate`, `TooFewItems`, an abort by rollback, and an opt-out | core (A3) |
| `promotion.json` | per-candidate score tables and their counts, eligibility bits and the decision, including the missing-evaluation rule, dropped items, anchors, both guards and ties | core (A8) |
| `pool.json` | a line's pool through deposits, φ, fees, escrow, S1, S2 caps, vesting, forfeit and dissolution, with the conservation sums | core (A9) |
| `scoring/*.json`, `pipelines/eval-*.json` | the scoring library (`consensus-vectors/tir-v2/`) | eval (A7, landed) |
| `composite.json` | composite roots and class ids | cand (A5) |
| `suite.json`, `judge.json` | a suite dataset's RFC 6962 roots and inclusion proofs (§17.6.3); a template's root, its fills and the judged score of parts (§17.8.5) | eval (A6, 2026-09-30) |
