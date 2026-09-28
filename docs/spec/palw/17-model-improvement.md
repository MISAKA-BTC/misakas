# PALW spec — 17. The Model Improvement Protocol

> **Normative.** This chapter specifies RFC-0004 (PALW Model Improvement Protocol, as of 62fd7558c):
> governed lines, the epoch state machine, the material an epoch collects, candidates and their
> composite artifacts, evaluation, promotion, rollback and rewards. It applies past the dormant fence
> `palw_improvement_v1`. Below the fence nothing in this chapter is read, both of its tables are
> empty, and every root and carriage is byte-identical to a build without it.
>
> Status: **skeleton (step 0)**. §17.0 — the tags and ids every lane builds against — is final for
> v1. The chapter's other sections are written by work item A1 and reviewed with the vectors. A
> number in §17.0 moves only by a change to this table, reviewed as a consensus change.

Contents: §17.0 tags and ids · §17.1 the fence · §17.2 state · §17.3 objects · §17.4 lines and the
policy · §17.5 the epoch state machine · §17.6 material · §17.7 candidates · §17.8 evaluation ·
§17.9 promotion · §17.10 rollback · §17.11 rewards and the pool · §17.12 rules PALW-MIP-1…20 ·
§17.13 vectors.

---

## 17.0 Tags and ids

Every number below was chosen free on every branch that touches the same space: `tir/fence-2`
(object tag 67, delta 88–89, tails `0xC0`–`0xC1`), `rfc3/fp-v5` (object tags 68–69, delta 90, tail
`0xC2`, court proofs 10–12, step fault 21) and the capacity line (tails `0xB*`).

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

Tag 83 is the next free tag.

**Until each object's admission lands, every layer treats all thirteen alike:** the stateless gate
(`palw_lifecycle_object_may_ride_v2`) lets each one ride at every height; the acceptance walk drops
each one by name, first and charged nothing — below the fence because an older build skips it, above
it because its admission has not landed; and the fold refuses each one as the second lock
(`PalwStateV2Error::ImprovementObjectRefused`). One function names them for all three
(`palw_improvement_object_name_v1`, with the predicate `palw_object_is_improvement_v1`). None is an
H-1 heartbeat carrier.

RFC-0004 §4.1's `LineageHeadSet` is not an object: the fold applies a promotion at `t_score`.
Evaluation claims are the existing claim objects with the evaluation job family (A6).

### State

| Item | Value |
| --- | --- |
| Table `improvement_lines` | `BTreeMap<Hash64 line_id, PalwImprovementLineV1>` |
| Table `improvement_epochs` | `BTreeMap<(Hash64 line_id, u64 epoch), PalwImprovementEpochV1>` |
| `state_root` block | label `improvement/v1`, then `collection_root("improvement_lines")` and `collection_root("improvement_epochs")`; hashed only when either table holds a row (after `gen_classes/v1`) |
| Delta entry 91 | `PalwDeltaEntryV2::ImprovementLine { key, old, new }` (rows boxed) |
| Delta entry 92 | `PalwDeltaEntryV2::ImprovementEpoch { key, old, new }` (rows boxed) |
| Writers | `TransitionBuilder::write_improvement_line(key, Option<row>)`, `write_improvement_epoch(key, Option<row>)` — the only writers |
| Carriage tail `0xC3` | `improvement_lines`, encoded only when non-empty |
| Carriage tail `0xC4` | `improvement_epochs`, encoded only when non-empty; every row's line must be governed |

### Court and step ids (reserved, not used by step 0)

| Space | Reserved | Last used before |
| --- | --- | --- |
| `PalwCourtVerdictProofV2` tags | 13 and up, for evaluation-claim proofs if A6/A7 need one | 12 (`GenDissection`, RFC-0003) |
| `PalwStepFaultV1` discriminants | 22 and up | 21 (`TirOutputDigestMismatch`, RFC-0003) |

Evaluation jobs are RFC-0003 pipeline jobs, adjudicated by the courts that already exist. A new proof
or fault is added only if A6/A7 show one is needed, and takes the next number here.

### The fence

| Item | Value |
| --- | --- |
| Field | `Params::palw_improvement_v1: Option<PalwImprovementFenceV1>` (`palw_improve_v1`) |
| Value | `{ activation, scoring_set_id, sign_table_id, court_version, ceilings: PalwImprovementCeilingsV1 }` |
| `consensus_params_id` | Some-only: `"palw_improvement_v1/protocol-v1"`, the activation (LE u64), then the value |
| `consensus_schedule_id` | Some-only: `"palw_improvement_v1"`, the activation, then the value |
| Value bytes | `scoring_set_id` (64) ‖ `sign_table_id` (64) ‖ LE u16 `court_version` ‖ u8 `max_candidates_per_epoch` ‖ LE u32 `max_items_per_epoch` ‖ LE u64 `max_eval_positions_per_epoch` ‖ LE u16 `max_eval_budget_permille` ‖ LE u32 `max_governed_lines` ‖ LE u32 `max_policy_bytes` |
| Normaliser | `Some(never())` collapses to `None`; `for_each_fence` visits the activation only |
| Fork id | listed in `palw_fences_v1` as `("palw_improvement_v1", activation)` |
| Mirror | `PalwStateParamsV2::improve_from_daa` (borsh-skipped), written by `Params::sync_palw_improvement_v1` only |
| Drill | `--palw-drill-improve-at` (`palw_drill_improve_fence_at_v1`, list `PALW_DRILL_IMPROVE_FENCES_V1`, ceilings `PALW_DRILL_IMPROVE_CEILINGS_V1`) |
| Format caps | `k_max` ≤ 64, `n` ≤ 2^16, positions ≤ 2^40 per epoch, budget ≤ 1,000 ‰, lines ≤ 2^16, policy ≤ 65,536 bytes; none zero |
| Prerequisites | `palw_audit_2026_09_11` declared; `palw_tir_v1`, `palw_gen_v1` and `palw_kary_court` in force at or below the activation; a `ConsensusV2` ruleset |
| Shipped | `None` on every preset and in no testnet-12 flag-day list |

### Domain keys (keyed BLAKE2b-512)

| Key | Used by |
| --- | --- |
| `misaka-palw/improve/scoring-set/v1` | `scoring_set_id` over the scoring library's descriptor (A7 fixes the descriptor) |
| `misaka-palw/improve/sign-table/v1` | `sign_table_id` over the pinned binomial table (A8 fixes the table) |
| `misaka-palw/improve/policy/v1` | a policy's digest |
| `misaka-palw/improve/eval-seed/v1` | an item's generation seed, `H(epoch seed ‖ LE u32 item)` |
| `misaka-palw/improve/eval-job/v1` | an evaluation job's id, `H(line ‖ LE u64 epoch ‖ LE u32 item ‖ borsh(subject))` |
| `misaka-palw/improve/composite-artifact/v1` | a composite candidate's artifact root, `H(parent_class ‖ parent_root ‖ adapter_root ‖ LE u32 P)` |
| `misaka-palw/improve/candidate-declarations/v1` | a candidate's declarations digest |

---

## 17.1 – 17.13

Written by A1. Until then RFC-0004 §3–§13 and its *Proposed Spec text* are the reference, with the
names and numbers of §17.0.
