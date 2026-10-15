> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 1999–2515 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): §3.10 J (attribution F1/F2), §3.11 DA (the DA court, redesigned).
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

### 3.10 J — attribution: F2 and F1, as the audit session's spec defines them

Owner **A**: M1 is F2 (SPEC §3), M2 is F1 including F1-M and F1c (SPEC §4 plus v3.1 decisions 1–2). **The audit
session's `f2f1_spec.md` is authoritative** for every name, type, discriminant, check order, ledger key and test in this
section. This section restates what the rest of this ADR depends on and adds only R-core's own rules, marked
**R-core**. Where v3 said otherwise, the spec wins (table "v3.1 changes", N1–N13). Naming: the spec's identity checks are
**J1–J7** (no hyphen; J6 and J7 are the addendum's, §4-bis.2); this ADR's rules are **J-1…J-8** (J-8 by post-edit 7).

**Why** (v3's reason, corrected). `job_id == claim_id`, which every execution-proving check requires today (OFF:440,
:478, :486, :493, :500), is a hash fixed point on **both** lanes: `attempt_id_v2` hashes the attempt, which carries
`execution_root`, which hashes the context whose `job_id` is the anchor (`produce.rs:189`); and `fp_claim_id_v3` hashes
the commitment, which contains `job` and `execution_root` (SPEC §0). v3's "on the FP lane `job_id == claim_id`"
(T61c) was false. Seats running kaspad sign only V3 receipts once Verification V2 is armed, and the old gate ran only in
the processor (`consume_objective_offence` never called it). Nothing recorded which job a claim's roots belong to (F1).

**J-1 (the job identity; M2; SPEC §4.1–§4.2).** Owner A (M2). Tests T-THREAD, T18g, `f1_job_identity_survives_reorg_across_admission`.
* `PalwClaimStateV2.job_identity: Hash64`, appended after `rights_reserved`:
  * **attempt:** the `execution_anchor_v3` of the carrying header, through `palw_execution_anchor_v1(header, attempt)`
    split out of `palw_execution_key_v1`; own work through `extras.own_job_anchor`, merged work through
    `PalwMergedWorkV1.job_anchor` from the blue's own header (`default` when the header is missing, never the
    `attempt_id` fallback), `PalwAttemptOriginV1.job_anchor`; `apply_attempt` writes `job_identity = anchor` when
    `execution_commitment_v3(attempt, anchor) == origin.execution_key`, otherwise 0;
  * **free prompt:** `palw_fp_job_pin_v1(commitment)`, carried as `FreePromptCommitted.job_pin`.
* **0 means not recorded:** it never convicts (`IdentityNotRecorded`) and never refuses admission (a refused own attempt
  fails the whole block). Written only when `offence_attribution_active`; otherwise 0.
* `PalwPanelLiabilityRecordV1` appends `job_identity`, `free_prompt: bool`, `trace_root`, `segment_count: u16` after
  `settled_at_final`, copied in `persist_panel_liability` (`segment_count = palw_segment_count_v2(panel.seats.len())`,
  or 0).
* **R-core:** `finalize_claim` copies the same four into the vesting row (V-1, N8).
* Withdrawn from v3: the `claim_rcore.job_anchor` field, its M1 placement (C6), the admission refusal of a zero
  anchor, and "an unloadable header refuses the block" (IMPL-13).

**J-2 (one resolution order).** Owner A (M1 for the spec's half; S for the row). Tests T62, T28, T46f.
* The spec's `palw_offence_target_v1(state, claim_id)` resolves the claim record, then the liability record, into
  `PalwOffenceTargetV1 { claim_id, class_id, artifact_root, executor_bond, execution_root, lane, segment_count, phase }`
  (SPEC §3.2–§3.3). No target: `NoTarget`.
* **F1 adds two target fields (R9)**, which J-5's checks read (`target.job_identity` in J1, J3 and J5,
  `target.trace_root` in J4). `PalwOffenceTargetV1` is the adjudicator's value, not Borsh and not state, so they cost no
  encoding:
  * `job_identity: Hash64` — the claim record's `job_identity` (J-1), else the liability record's (F1's appended
    field), else, past `palw_rcore_plus`, the vesting row's copy; 0 when none recorded it (`IdentityNotRecorded`);
  * `trace_root: Hash64` — the claim record's `trace_root` (`PalwClaimStateV2.trace_root`), else the liability record's
    (F1's appended field), else the vesting row's copy.
* **R-core:** past `palw_rcore_plus` the vesting row is the third source (lane from `free_prompt`, `segment_count`,
  `trace_root`, `job_identity` and `artifact_root` from the row). Liability records are not pruned while a row exists
  (X29), so the row is a fallback that T28 exercises by pruning in a fixture, not a path the live chain expects.
* Every DA answer (DA-4) and every held-DA check resolves the same target (SPEC §4.6).

**J-3 (the adjudicator; F2; M1; SPEC §3.1–§3.5).** Owner A (M1). Tests T46a–T46n, `palw_offence_attribution_is_t12_only`,
`palw_offence_attribution_t11_verdicts`.
* **Fence.** `Params::palw_offence_attribution: Option<ForkActivation>`, Some-only in every writer (the
  `palw_clock_floor` pattern), t12 only, DAA 0 only; `validate_palw_v2` requires `palw_objective_offence`,
  `palw_audit_2026_09_23`, `palw_verification_v2` and `palw_economic_safety`, and (F1) `palw_prefill_draw` at 0. The
  extras carry `PalwTransitionExtrasV1::offence_attribution_active`.
* **Kind and evidence.** `PalwOffenceKindV1::PanelFalseValidV2 = 3`, appended; evidence
  `PalwPanelFalseValidEvidenceV2 { version: u16 /* 2 */, claim_id: Hash64, accused_seat: TransactionOutpoint,
  receipt: PalwFalseValidReceiptV1, contradiction: PalwPanelContradictionV1, reporter_reveal: Vec<u8> }` with
  `PalwFalseValidReceiptV1 { Full(PalwSeatReceiptV2) = 0, Segmented(PalwSeatReceiptV3) = 1 }`. No `network_domain` (the
  processor uses the chain's), no `executor_pubkey` (keys come from state). `reporter_reveal` must be empty until F7's
  fence and is outside every conviction hash; SPEC gives it no size bound of its own (the draft's "at most 256 bytes"
  is withdrawn, R10: on t12 the slot must be empty, and F7 sets its bound when it arms it). The whole evidence is capped
  at `PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES`.
* **One adjudicator,** `palw_check_panel_false_valid_v2(state, accused, evidence, fp_decode_rules_active,
  reporter_armed, sig)`, called by the processor with `sig = Some(..)` and by the fold's `consume_false_valid_v2` with
  `None`. In order: the byte cap; decode, `version == 2`, the reporter slot; `accused_seat == accused ==
  receipt.seat_bond`, `inner.claim == claim_id`, verdict `Valid`; the signature (a `Full` receipt under the V2 message
  and context, a `Segmented` one under the V3 message with its segments; the seat Active or Retiring); the target
  (J-2); `ClaimUnderSession`; the admission table; for the execution-proving kinds
  `palw_panel_contradiction_convicts_execution_v1(c, target.execution_root, target.artifact_root, ladder)`, with the
  ladder `state.class_step_ladder_v1(class, PALW_FALSE_VALID_NETWORK_LADDER_V1)`, never 64. **It binds by root. It
  never compares `claim_id`.**
* **The finding (post-edit 2; ADDENDUM §4-bis.7, landing with M2).** `PalwFalseValidFindingV1 { target, site,
  execution_proving: bool }` becomes `{ target, site, acts_on_claim: bool, forfeit: PalwForfeitScopeV1 }` with
  `PalwForfeitScopeV1 { None, ByClaim, ByRoot }`. The admission table gains a `ClaimProving` class for 9, 10 and 13 and
  puts 11 and 12 under `ExecutionProving`; `acts_on_claim` is true for both classes (the claim is voided or its Final
  reversed), `forfeit` is `ByClaim` for `ClaimProving` and `ByRoot` for `ExecutionProving` (V-2b). Kind 1 (V1) payloads
  carrying tags 9–13 are refused right after decode, as `PanelFalseValidNeedsContradiction` refused them before (V1
  parity, ADDENDUM §4-bis.7).
* **Admission.** Admitted: `ProducerWithholding` and `CourtFraud` (tied to state through the shared extraction of
  `bind_panel_false_valid`; N9 narrows `ProducerWithholding`), `StepArithmetic`, `StepStructural`, `ForgedOutput`, and
  (F1) 9–13 (13 = `PromptNotAnchored`, post-edit 1). Refused by name: `ExecutorEquivocation`, `CourtExecutorGuilty`, `ConflictingPermit`, `Legs`. Past the fence
  kind 1 is refused (`SupersededOnThisNetwork`), which closes the V1 licence route and the forced-id route.
* **Site and mask liability (SPEC §3.3 step 9).** A `Full` receipt is always liable. A `Segmented` receipt with mask
  `m ≠ NONE` is liable for a `Whole` site only with `segment_count = Some(k)` and `m.is_full(k)`, and for `Leaf(l)` only
  with `Some(k)` and `m` full or covering `palw_segment_index_of_leaf_v2(binding.step_leaf_count, k, l)`; otherwise
  `SiteNotAttested` or `SegmentsUnknown`. The mask is authentic because coverage licences require every `Valid` mask to
  be the assigned one.
* **Effect (SPEC §3.5).** Ledger key one per (seat, claim); the lock is taken and removed first; `slash_bond`; the
  consumed row has kind 3 and records the target's root only for a `ByRoot` fault with economic safety armed; a
  finding that `acts_on_claim` voids a live claim `void_and_slash(CourtFraud)` or reverses a Final
  (`reverse_convicted_final`, `mark_liability_convicted`).
* **`ClaimUnderSession` refuses only on an open court (post-edit 2, amending SPEC §3.3 step 6).** For kinds 3 and 4,
  the adjudicator refuses with `ClaimUnderSession` only when `open_courts_of(claim) > 0`; an open DA session, held-DA
  (`held_da_missing`) or R-core's, never blocks a conviction. Voiding under a DA session is safe (held-DA rows are
  dropped at the claim write, the accuser reservation released; ADDENDUM §1), and the change removes the "open a DA
  session to delay a conviction" lever. This holds from M1/M2 on, below and above `palw_rcore_plus`; N13 adds what
  R-core does with its own sessions.
* **R-core, past `palw_rcore_plus`:**
  * the tiers are §3.6's: S4 on the signer; S2 (before Final) or S3 (with an unmatured row) on the producer, once per
    claim, through the void or reversal the adjudicator triggers; `collected` recorded (R-2); round rights forfeited by
    V-2b's route;
  * **`ClaimUnderSession` (N13)** applies to open courts only (as it now does on every t12 build, post-edit 2). R-core's
    DA sessions live in side maps and never change the claim's phase (DA-1, DA-2), so an open session does not block a
    conviction; the conviction closes every open DA session of the claim (exposure returned) and refunds every
    `refuted_held` entry (DA-6) in the same funnel. (Below `palw_rcore_plus` a held-DA session no longer blocks either;
    the v3.1 text that kept `held_da_missing` there is withdrawn.)
  * the S3 sampler question (SPEC §5 item 3) is answered by Q-1: a sampler signs `Sampled`, not `Valid`, so the
    adjudicator's verdict check never makes it liable;
  * DA-7's S4 on covering signers is written under the same (seat, claim) key (N9).

**J-4 (`ExecutorRefuted`; M2; SPEC §4.4).** Owner A (M2; the post-Final charge in S). Tests T18b, T18c, T81.
* `PalwOffenceKindV1::ExecutorRefuted = 4`, carried by `ObjectiveOffence` (tag 51); no new object tag. The accused
  must be `target.executor_bond`; no receipt.
* Evidence `{ version: 1, claim_id, contradiction ∈ {5, 6, 8, 9, 10, 11, 12, 13}, reporter_reveal }` (StepArithmetic,
  StepStructural, ForgedOutput, IdentityMismatch, OutputMismatch, ForgedOutputTiled, LogitsNotStepOutput,
  PromptNotAnchored; 13 by post-edit 1, ADDENDUM §4-bis.7). Ledger key
  `palw_offence_id_v1(ExecutorRefuted, executor, H("misaka-palw/executor-refuted-key/v1" ‖ claim_id))`: one per claim.
* Effect by phase (an open DA session does not block it, post-edit 2): live, not terminal, no open court →
  `void_and_slash(CourtFraud)`, recording the actual debit; Final
  → `reverse_convicted_final` and `mark_liability_convicted`; voided or retired → record and mark the row. Signers follow
  through kind 3 (`CourtFraud{voided_daa}` or the same contradiction).
* **R-core:** S2 before Final and S3 with an unmatured row (§3.6); the executor's post-Final charge that the spec leaves
  to "SR-8" is §3.6's S3 (row burn plus `min(25%·C, 3G)`), landed in S; an FP claim, which has no row, takes §3.6's
  S3-FP (U3, post-edit 12). Withdrawn from v3: `ExecutorRefuted = 3`,
  `PalwExecutorRefutedEvidenceV1 { .., network_domain, .. }`, `Legs` as an accepted kind, and the
  `H(claim_id ‖ contradiction digest)` key.

**J-5 (the identity rule and contradictions 9–13; M2; SPEC §4.3–§4.4, ADDENDUM §4-bis).** Owner A (M2). Tests T18b,
T18d, T18e, T18f, T18h, T18m, T18p, T18c(iv), T18q–T18y, the Tier B golden.
* `palw_binding_identity_fault_v1(target, binding, net_form, base_class_id)` **refuses** when `verify_binding_v1` fails,
  when `committed_execution_root != target.execution_root`, or when `job_identity == 0`. It reports a **fault** on:
  * **J1** — attempt: `job_context.job_id != job_identity`; FP: `palw_fp_job_pin_of_context_v1(ctx) != job_identity`;
  * **J2** — `shape_profile.shape_profile_id() != class_id`;
  * **J3** — attempt: `execution_seed != job_identity[..32]`;
  * **J4** — `full_logits_trace_root != target.trace_root`;
  * **J5** — floor attempts: the whole context differs from `palw_floor_attempt_context_v1(profile, job_identity,
    PALW_RC_BASE0_CANONICAL, net_form, prefill_draw = true)`, the seats' own rule (`backend.rs:565-576`), with the floor's
    canonical job moved into core byte for byte.
* Contradictions, appended with fixed discriminants: `IdentityMismatch { binding } = 9` (site, Q-6 and ADDENDUM
  §4-bis.7: J1/J2/J3/J5a/J5b at `AnyValid` only if SEAT-S1, SEAT-S3 and SEAT-S4 ship and T18p-M is GREEN before the
  regenesis, else `Whole`; J4/J6/J7 at `Whole`); `OutputMismatch { binding, pin } = 10` (`Whole`; the addendum sets
  the pin to `PalwDecodeTokenPinV1`, and after post-edit 4 it covers attempts on every class **and FP model claims**,
  below); `ForgedOutputTiled { binding, proof } = 11` (F1-M);
  `LogitsNotStepOutput { event: PalwTraceEventDisclosureV1, row: u32, head_tile: u32, head_opening: PalwStepOpeningV1 } = 12`
  (F1c, the addendum's **hash form**, §4-bis.6: the event carries the binding, and `head_opening` is the committed head
  leaf and its path, with no preimage; the spike's preimage form `{ binding, pin, position, tile }` is superseded,
  ADDENDUM §2); **`PromptNotAnchored { binding, proof: PalwPromptProofV1 } = 13`** (post-edit 1, below). The V1 paths
  return their old `NeedsContradiction` refusal for tags 9–13.
* **`PromptNotAnchored = 13` (post-edit 1; ADDENDUM §4-bis.3).** Appended; attempt lane only; for a class whose
  canonical prefill is above `PALW_J5_INLINE_PROMPT_IDS_V1` = 4,096 ids (the 2M row), where J5b's inline recompute is
  too costly. `PalwPromptProofV1 { Tile(PalwPromptIdsOpeningV1) = 0, Whole = 1 }` (`use_discriminant`).
  * Checks, in order: the common prefix (byte cap, `verify_binding_v1`, the root pin, `job_identity ≠ 0`); J2 is a
    **refusal** here (file 9 instead); the lane is Attempt; the canonical job exists and the context's declared prefill
    equals it (otherwise file 9's J5a); the prefill is above 4,096 (otherwise 9's J5b); the class's prompt-ids form is
    `MerkleV1`.
  * `Tile(o)`: the opening must verify against `ctx.prompt_token_ids_hash`; the fault is a tile whose ids differ from
    `palw_attempt_prompt_ids_range_v1(job_identity, vocab, 32·o.tile_index, len)`.
  * `Whole`: the whole prompt root recomputed by `palw_attempt_prompt_root_v1` differs from the context's. **It is
    charged against the block's heavy budget `PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1` = 2^18 ids before it is computed**
    (one 2M check per block); an exhausted budget refuses it without computing. The processor keeps the counter in its
    per-object acceptance loop (the object is dropped, the block stands) and the fold mirrors it in the transition
    builder. `Whole` is the only route for a 2M prompt root that has no preimage after Final.
  * Forfeiture by claim (V-2b); site `AnyValid` only if SEAT-S1, SEAT-S3 and SEAT-S4 ship and T18p-M is GREEN before
    the regenesis, otherwise `Whole` (ADDENDUM §4-bis.7); kind 4 admits it (J-4).
* **Admission under the fence (post-edit 4; ADDENDUM §4-bis.8, processor only).** Past `palw_offence_attribution` a
  class registration is refused unless: (a) its canonical job equals `palw_attempt_canonical_v1(profile, false)`, that
  is **`(n_ctx/8 − 1, 2)` for model classes**, so registrants lose the choice of canonical job (operator decision);
  (b) `palw_logits_head_v1(profile)` exists (which also refuses Float32); (c) **it reaches no Kimi-K3 kernel**
  (operator decision: Kimi-kernel classes are refused at admission; no Kimi engine or golden identity exists, and t12
  arms `palw_kimi_k3`); (d) a canonical prefill above 4,096 uses the `MerkleV1` prompt-ids form. The genesis rows
  satisfy (a)–(d) (T18w).
* **The FP `output_root` rendered rule is unified (post-edit 4).** The addendum's default left model-class free-prompt
  `output_root` as a residual for 10 (its Q6); the operator chose to unify the rendered rule, so `OutputMismatch` (10)
  also convicts FP model claims. This moves FP roots and the `misaka-palw-derive` tool once, at this regenesis (§6,
  "Genesis").
* **F1-M is in the gate (v3.1 decision 1).** The relabel must be convictable on 8k and 2M, not only on the floor, and
  `ForgedOutputTiled` must convict a forged token on tiled/A16 decode. Model-class production starts at genesis (no
  model class held back at start). **The function and its inputs (R4) are the audit's addendum, `f1c_f1m_spec.md`
  §4-bis, authoritative as SPEC is:**
  * **One core attempt rule, `CoreV1`** (new `core/palw_attempt_rules_v1.rs`, §4-bis.1), for the floor and every model
    class: `palw_attempt_context_v1(profile, anchor, (p, d), prompt_hash)`, with the canonical job
    `palw_attempt_canonical_v1(profile, is_base)` (the base class `(8, 4)`; otherwise `(f − 1, 2)` from the profile's
    footprint floor, which equals today's held canonical `(n_ctx/8 − 1, 2)`, now fixed for every model class by the
    admission rule above), and the prompt root
    `palw_attempt_prompt_root_v1(profile, anchor, p, net_form)` over the floor's counter-mode prompt loop moved into core.
    On the floor it is F1's `palw_floor_attempt_context_v1` byte for byte (golden test).
  * **What J5 compares, and from where.** J5a: the whole `context_hash` against `CoreV1`'s, from the class's shape
    profile (J2 has already tied the binding's profile to `target.class_id`, a chain fact), `target.job_identity` (J-1)
    and the formula canonical; J5b (prompts of at most 4,096
    ids: the floor, the 8k row): the prompt root recomputed from the same three. Above 4,096 ids (the 2M row) the
    prompt root is checked by `PromptNotAnchored = 13` (above). The addendum also adds J6 (the activation-leg root)
    and J7 (the checkpoint profile) to 9 (§4-bis.2). **No field held only by an
    artifact enters the check:** under `CoreV1` the model, runtime, tokenizer, nullifier, assignment and cu fields are
    0 and `max_context_tokens = n_ctx`, and producers and seats switch their `job_for_anchor` to `CoreV1` under
    `palw_offence_attribution`, which moves model attempt roots once, at this regenesis. **No family mapping is
    needed:** one rule serves every family; the per-family prompt generators and execution domains
    (`qwen25_a16_prompt_for_anchor`, `qwen36_prompt_for_anchor`) are what `CoreV1` replaces on t12.
  * **Fixtures (a consensus test cannot load a production 8k or 2M artifact).** T18e and T18m run in
    `consensus/src/pipeline/virtual_processor/tests/t47_model_class_attribution.rs` on the A16 held v7 and Qwen3.6 v7
    fixture families (`RelabelPrompt` drill fault; a 2M-sized profile for the metered route), and the Tier B golden
    `attempt_rules_core_v1_golden.rs` pins `job_for_anchor` under `CoreV1` against the real 8k, 2M and Q36@512
    profiles (context only).
* **F1c is in the gate (decision 2).** `LogitsNotStepOutput` must convict T18c(iv), garbage logits on an honest floor
  step tree. **The spike is done** (addendum §1: the committed logits row equals the head's step output bit for bit on
  the floor, A16 held v7 including ragged vocab 8,292, and Qwen3.6; probes re-run), so contradiction 12 takes the
  addendum's **hash form** (§4-bis.6: the derived head leaf against the committed `head_opening`). **If a family fails
  the head predicate** `palw_logits_head_v1`, its registration is refused under `palw_offence_attribution`
  (§4-bis.8(b)), so no admitted class is outside F1c; the genesis rows pass (T18w). T18c(iv) asserts a conviction by 12
  under kind 4 (Tier A) and on a real floor attempt (`t46o`–`t46q`).
* 9 and 10 cannot convict an honest producer whatever Q1's answer: an honest binding is its own job's and its own
  output's. The arithmetic kinds on float lanes still rest on runtime determinism (precondition v).
* **Where the identity rule also runs (SPEC §4.6).** The DA answers. M2 adds the rule to the v1 answer objects, which is
  what SPEC T18b exercises before S lands. Past `palw_rcore_plus` the v1 answer objects are refused (DA-1) and
  `MaterialDisclosedV2` (DA-4) runs the same function on the binding it discloses (M3's T18b-R; SPEC T18b stays as its
  fence-off twin, J-7); a faulty binding is not an answer (the session defaults, S1), and `IdentityNotRecorded` falls back to the old checks. The held checkers take
  `Option<&PalwOffenceTargetV1>`; an accusation carrying a faulty binding is refused and its filer sends kind 4.

**J-6 (the unique path; F1).** Owner A (M2; the DA legs in M3). Tests T62, T18c, T66. Every conviction follows one chain
of chain facts:

| step | value | recorded by | checked by |
|---|---|---|---|
| claim | `claim_id` | the object | J-2 |
| committed root | `target.execution_root` | admission (the attempt field) | J-3 (`verify_binding` rebuilds the root) |
| job identity | `target.job_identity` | admission (J-1) | J-5 (J1–J5) |
| challenged index | a DA unit (named or drawn), a StepLeaf, or the contradiction's leaf or position | DA-3, the object | DA-4, the verifier |
| accused signer | the producer (`ExecutorRefuted`, DA default), or a `Valid` signer liable by mask and site | the receipt (J-3), the lock (L-3) for DA-7 | J-3, DA-7 |
| objective fault | arithmetic, structural, forged output (plain or tiled), identity, output, logits-not-step-output, prompt-not-anchored (13), or a DA default | the evidence or the sweep | J-4, J-5, DA-7 |
| slash target | the tier in §3.6 | | §3.6 |

* **Garbage path** (the correct job, a garbage trace): an event DA answer reveals the binding (J1–J5 pass) → a held
  `StepLeaf` demand (`PalwHeldMissingV1::StepLeaf`) at a leaf the seat's own replay shows divergent → the producer
  discloses the leaf's evidence or defaults (S1) → `StepArithmetic` or a held dissection → `ExecutorRefuted` (S2 before
  Final). Garbage logits over an honest step tree → `LogitsNotStepOutput` (F1c). Past `palw_rcore_plus` the named leaf is
  free and keyed by the session (DA-3, IMPL-12). **Open gap (post-edit 8, gates launch):** when the divergent leaf is
  an `AttnFused` step leaf of a held-context class (8k, 2M), the checker returns `NeedsDissection` and no route
  convicts the arithmetic lie today (§4.2 #18, §8.3 item 7). **Until M3 lands, T18c's `StepLeaf` step runs under ADR-0111 D3** (one
  demand per seat per claim, inside the seat's draw sample), so T18c at M2 names a leaf inside the demanding seat's sample.
* **Borrowed path** (another job's roots, or a relabelled honest run): any DA answer fails the identity checks →
  default (S1). Or anyone who holds the binding (for example a gossiped capture, PANEL:3361-3368) files
  `ExecutorRefuted{IdentityMismatch}` directly; the record's root is 0 and the forfeiture is by claim (V-2b), so the
  honest lender's claim, lock, schedule rows and rights are untouched (SPEC T18b).

**J-7 (what GREEN means).** Owner A. Tests as below.
* **M1 (F2) GREEN** is SPEC §3.6's criterion: T46a–T46n pass through the processor gate **and** the fold on
  producer-built claims (in `consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs`, the one crate
  with both the producer and the gate, which replaces v3's T60 home), the two parity files pass, and the existing suites
  are unchanged.
* **M2 (F1) GREEN** is SPEC §4.7's criterion — T18b, T18c(i–iii), T18d, T18e (floor), T18f, T18g, T18h, T18k, T18p,
  T-THREAD, the Tier B golden, reorg/restart and the v22 goldens — **plus v3.1's gate items:** T18e convicts on 8k and 2M
  and T18m passes (F1-M), and T18c(iv) convicts (F1c). The spec's two residual pins become convictions before the gate.
  **The post-edits add** the addendum's T18q–T18y (13's heavy budget is T18u, the admission rule T18w, a claim under a
  DA session voided by kind 4 T18y), and T18p-M, which decides the `AnyValid` sites (J-5) and gates SEAT-R (Q-7).
* **T46n, restated (post-edit 2).** SPEC's T46n asserted that an open court **or a held DA session** gives
  `ClaimUnderSession`. With the amendment, T46n asserts that an open court refuses kinds 3 and 4 with
  `ClaimUnderSession` and writes nothing, and that a held DA session (`held_da_missing`) does **not**: the conviction
  lands (with the addendum's T18y). This holds with `palw_rcore_plus` off and on.
* **SPEC tests that run the v1 DA paths, once M3 lands (R6).** SPEC T18b (a `MaterialDisclosed` refused by the identity
  rule, then `ProducerWithholding`), T18c(ii) (`StepRange`/`StepLeaf` silence through the held DA path) and T46n's held
  DA case (post-edit 2: `held_da_missing` **not** blocking the conviction) build fixtures that DA-1 refuses or no longer
  writes past `palw_rcore_plus`. They are kept (T18b and T18c(ii) unchanged, T46n as restated by post-edit 2 above) on
  the t12 harness with **`palw_rcore_plus` forced off**
  (`palw_offence_attribution` stays on, the M1/M2 state), where they pin the v1 paths as fence-off twins, as this ADR
  requires of every dormant branch. M3 adds their R-core twins, which run with the fence on:
  * **T18b-R:** the borrowed-root claim's DA session is answered by a `MaterialDisclosedV2` carrying R's binding; the
    identity rule refuses it as an answer, the session defaults (DA-7), and the claim voids with a `DaDefault` record
    (root 0, forfeiture by claim); kind 4 `IdentityMismatch` on C2 and C0's byte-identity are asserted as in SPEC;
  * **T18c(ii)-R:** a root with no preimage: a drawn or named held unit (`StepRange` of width 1, or a `StepLeaf`) is not
    answered, and the session defaults (S1, `DaDefault`);
  * **T46n-R:** an open **court** still refuses kind 3 and kind 4 with `ClaimUnderSession` and writes nothing; an open
    R-core DA session does not: the conviction lands, closes every open session and refunds `refuted_held` (N13,
    DA-6; with T69).
* v3's T60, T61 and T63 are retired into these (§8.1).

**J-8 (courts under the fence; post-edit 7).** Owner A. Launch line only: never patched onto the live t12 fleet.
Tests: the audit's, on `8be0f661` (`fix/t12-shard-court-openings-first`); T18v for the court door.
* **The shard court's one move.** Under `palw_offence_attribution` a one-move verdict goes through
  `palw_one_move_verdict_bound_v2`, which verifies the openings before it can return `NeedsDissection`; and
  `NeedsDissection` is **refused** for classes whose root evidence cannot be built (the held-context classes), so a
  one-move court never parks a claim on a dissection nobody can complete. The shard and checkpoint courts still void
  `CourtFraud` in one move with no session (V-2b's per-claim `CourtConviction` key).
* **The seat-side ladder fix** passes the class ladder (`class_step_ladder_v1`), not the V1 default of 64, as the F2
  adjudicator already does (J-3).
* **The court door (ADDENDUM §4-bis.9, part of M2).** Under the same fence a `DecodeToken` / `DecodeTokenTiled` close is
  refused unless its narrowed leaf is the call's head slot (so a held dissection narrowed at an `AttnFused` leaf cannot
  close through it), and a decode arm's `NoFaultFound` is `DecodeCloseCannotAcquit`, not `ChallengerDefeated`. Below the
  fence the court is byte for byte unchanged. This removes a wrong acquittal; it does **not** give the `AttnFused` lie a
  conviction route (post-edit 8, §4.2 #18).

### 3.11 DA — the data-availability court, redesigned (F3, D3)

Owner **A**, M3, after M2 and after the skeleton (S). Tests T64–T69, T32, T34, T38. Every rule below also names its
own tests.

**DA-1 (scope).** Owner A (M3). Tests T34, T64. Past `palw_rcore_plus`, `DefaultAccused` (event units) and
`DefaultAccusedHeld` (held units) open sessions in side maps, on every class, floor included (C-1). A session never
changes the claim's phase: `DefaultDisputed` is never written past the fence, and `assert_internal_consistency_v3`
refuses it there. The v1 answer objects `MaterialDisclosed` and `MaterialDisclosedHeld` (producer-signed only) are
refused past the fence; `MaterialDisclosedV2` (DA-4) replaces both. The held court's side maps `held_da_missing` and
`held_leaf_demands` are not written past the fence: the accused unit lives in the session (IMPL-12). Below the fence
ADR-0062 SA-1…SA-7 and ADR-0111 D3 stand verbatim.

**DA-2 (state; schema v22).** Owner A (declared in S, written in M3). Tests T41, T40.

```rust
/// Keyed by (claim_id, accuser): one open session per accuser per claim.
pub struct PalwDaSessionV1 {
    pub opened_daa: u64,
    pub deadline_daa: u64,          // opened_daa + W_disclose (= window_challenge = 1,200 on t12)
    pub accuser_is_seat: bool,      // a seat of the claim's current panel when it opened; only these pause (DA-5)
    pub exposure: u128,             // DA-6; on the accuser's free half (A-6)
    pub units: Vec<PalwDaUnitV1>,   // [named, drawn...] (DA-3)
    pub stage: PalwDaStageV1,       // the claim's stage when it opened
}
pub enum PalwDaUnitV1 { Event { row: u32, tile: u8 }, Held(PalwHeldMissingV1) }
pub enum PalwDaStageV1 { Live, Licensed, FinalRow }

/// Keyed by claim_id. Exists from the first session until the claim record retires.
pub struct PalwDaClaimV1 {
    pub open_seat_sessions: u8,
    pub open_other_sessions: u8,
    pub opened_non_seat_total: u16,                  // lifetime, capped at 16 (DA-8)
    pub opened_by_seat: BTreeMap<PalwBondKeyV2, u8>, // lifetime per seat, capped at 4 (DA-8)
    pub paused_since: Option<u64>,                   // first SEAT session opened; cleared when the last seat session closes
    pub last_closed_daa: Option<u64>,                // DL-1: the retirement re-arm
    pub answered: BTreeSet<PalwDaUnitV1>,            // answered on chain for this claim; answers every session
    pub flat_answered: bool,                         // a Flat answer covers every in-run event row (DA-4)
    pub refuted_held: Vec<(PalwBondKeyV2, u128)>,    // refuted exposure held until the claim resolves (DA-6)
}
```

Rooted maps `da_sessions: BTreeMap<(Hash64, PalwBondKeyV2), PalwDaSessionV1>` and
`da_claims: BTreeMap<Hash64, PalwDaClaimV1>`. A derived index by `(deadline_daa, claim_id, accuser)`, delta-maintained and
rebuilt on load.

**DA-3 (units: named plus drawn; V3S-07, IMPL-12, IMPL-16).** Owner A (M3). Tests T64, T65, T66.
* **The named unit.** The accuser names one unit: the chunk or unit its fetch failed on (its `Unavailable` receipt),
  or a divergent step leaf it found by its own replay (J-6).
  * Past `palw_rcore_plus` a named `StepLeaf` is **free**: any leaf inside the binding's committed bound, not only
    one in the seat's ADR-0111 D3 draw sample. The acceptance layer's `palw_leaf_demand_is_the_seats_v1`
    (`palw_leaf_evidence_v1.rs:144`) does not apply to DA sessions, and the once-per-seat rule (`held_leaf_demands`)
    is replaced by the per-seat session budget of DA-8. Non-seats may name a `StepLeaf` too, within their caps.
  * A named leaf whose checker returns `NeedsDissection` (a fused-attention leaf) is refused as a DA unit
    (`DaUnitNeedsDissection`); its filer opens a held dissection (the court) instead. It is never a DA default. On a
    held-context class that route does not yet convict an `AttnFused` arithmetic lie (post-edit 8, §4.2 #18; a launch
    gate).
* **Drawn units.** At opening the fold draws up to `PALW_DA_DRAWN_UNITS_V1` = 3 more distinct units, seeded by
  `H(PALW_DA_DRAW_DOMAIN_V1 ‖ ctx.block ‖ claim_id ‖ accuser)`, where `ctx.block` is the block accepting the accusation
  (`PalwBlockContextV2::block`). **Every drawn unit is inside the committed run:**
  * **Event family:** rows uniform in `[0, decode_rows)`, where `decode_rows` is the committed run's decode count:
    for an attempt claim `palw_attempt_job_v1(canonical, prefill_draw_active_at(attempt block)).exact_decode_tokens`
    (1 on t12: the RC base arms `palw_prefill_draw` at 4,000, `params.rs:14946`, and t12's pass 2 moves every fence height to 0, `for_each_fence` in `palw_t12_arm_every_rule_from_genesis`; T64 pins it), for an FP claim its
    committed `exact_decode_tokens`; tiles uniform over that row's tile count. The draft drew rows over
    `palw_da_max_accusable_rows_v1(trace_chunk_count)` = 256, so on the one-row attempt run almost every drawn unit
    was answered `OutOfRange` from the binding alone (each draw hit the row with p = 1/256), and D3's "multiple
    indices" added almost nothing. If the run has fewer than 3 further distinct units, fewer are drawn.
  * **Attempt claims of held-context classes** (the 8k and 2M rows; the artifact's held map, not the panel-room hold) draw **held** units (`StepLeaf`, `StepRange` of width 1, `(checkpoint, chunk)`)
    within the bound the binding commits, not event units.
  * **Held family** (a `DefaultAccusedHeld`): units of the named unit's kind, uniform within the bound the
    accusation's binding commits (prompt tiles; `(checkpoint, chunk)`; `StepRange` of width 1 for `StepRange` and
    `StepLeaf`).
* The accuser cannot choose the draw. A miner that carries its own accusation can grind the draw only at one block's
  proof of work per try, and only for sessions it carries (accepted, §9.1 Q8).
* A `DefaultAccusedHeld` whose binding fails the identity rule (J-5) is refused (`DaBindingIsIdentityFault`): its filer
  should send `ExecutorRefuted{IdentityMismatch}` instead (SPEC §4.6).
* The helper `palw_da_accusation_admissible_v2(state, params, claim, accuser, now)` is exported for Phase 2's fee-safe
  de-duplication (C-8).

**DA-4 (answers).** Owner A (M3). Tests T32, T64. New object `MaterialDisclosedV2 { claim: Hash64, unit: PalwDaUnitV1,
answer: PalwDaAnswerV1, discloser: PalwBondKeyV2, signature: Vec<u8> }` (tag 55), with
`PalwDaAnswerV1 { Event(PalwTraceEventDisclosureV1), Held(Box<PalwHeldDisclosureCarriageV1>) }`.
* Signed by `discloser` over `palw_da_disclosure_message_v4(network, claim, unit, answer_digest, discloser)` under a
  new ML-DSA-87 context (§6, `COMPLETE_V5`).
* The discloser is the claim's producer or any bond with a live lock on the claim (X7, C-2).
* Checked by the existing hash arithmetic against the record's roots (`check_trace_event_disclosure_v1`, the held
  checkers), plus the identity rule J1–J5 on the disclosed binding (J-5, SPEC §4.6); `IdentityNotRecorded` falls back
  to the hash arithmetic alone.
* A unit that no open session demands is refused, which bounds the bytes.
* An accepted answer adds the unit to `answered`. **`flat_answered` (IMPL-16)** is set by an accepted `Flat` answer
  and covers every **in-run** event unit (`row < decode_rows`, tile 0), exactly the rows the Flat checker accepts
  (`palw_step_refute.rs:3047-3055`: a row ≥ decode is refused there and answered by `OutOfRange`). An out-of-run named
  unit is answered by `OutOfRange` from the binding alone.
* A session is **refuted** when every one of its units is answered: its exposure moves to `refuted_held` (DA-6) and
  the session closes.

**DA-5 (pause credit, re-keying and locks; V3S-02, V3S-04, V3S-08).** Owner A (M3). Tests T67, T66, T16, T82, T40.
* **Only a seat session pauses a pre-Final claim (V3S-08).** When the claim's open **seat** count goes 0 → 1:
  `paused_since = now`, and DL-1 gives the claim no deadline. A non-seat session never pauses a claim. So a producer's
  own non-seat Sybil cannot stretch its claim's room occupancy by answering at `deadline − 1`.
* When the open seat count goes 1 → 0 (by refutation, default or release): `shift = now − paused_since`.
  * `Provisional` (after a redraw): `rebound_daa += shift`. `PanelBound`: `bound_daa += shift`.
    **`ReceiptLicensed`: `licensed_daa += shift`.** v2 kept `licensed_daa` (STATE:14860ff at `e93be0f2`), so an
    offender that answered at L + 1,200 went Final in the next block (review L34-2). Now the claim gets back exactly
    the challenge time it had left.
  * The deadline is DL-1's value from the shifted anchor.
* **A Final row is re-keyed when a session opens (V3S-02).** On opening any session (seat or not) on a claim whose row
  exists: `row.expiry_daa = max(row.expiry_daa, deadline_daa + window_challenge_at)`, re-inserted in the ordered
  index in the same funnel. So the row sits behind every row that can mature before its session ends, the ordered
  scan of V-7 stays monotone, and V-4(c) is implied by V-4(a). The draft moved the expiry only at close, so one
  session on the head row stopped every later row (two refuted sessions, 640.20 MSK burned, froze all moves for
  about 2,252 DAA, 0.284 MSK/DAA; verifier arithmetic, reproduced).
* **The locks follow the row (V3S-04).** The same funnel re-dates every live lock of the claim's `Valid` signers to the
  row's new `expiry_daa` (L-3). A conviction that lands while a session is open, or within `window_challenge_at` of
  its end, still finds every lock live, so S4 lands.
* **Retirement.** A terminal claim's retirement is deferred while any session on it is open, and re-armed at
  `max(terminal_daa + claim_retirement_daa, last_closed_daa + 1)` (DL-1).
* **A redraw never closes a session.** A session opened on the first panel keeps its `accuser_is_seat` flag.
* **Bounded:** every session must fit the claim's retention (DA-8), so the total pause is at most
  `trace_retention_daa − accepted_daa` = 5,400 DAA on t12 (`bind + receipt + challenge + court`).

**DA-6 (what a session costs; D5, F9, C4, V3S-06).** Owner A (M3). Tests T69, T84.
* `exposure = min(⌈r × S_P(stage)⌉, min_collateral_sompi)`, r = 1,000 bps, where `S_P(stage)` is the producer's
  nominal debit that a default at the session's stage triggers, which is the DA reward's base (R-1):
  * `Live` and `Licensed`: `w + esc + rr` (after a released escrow, X7 takes `E` from uncommitted stake, so the total
    is the same);
  * `FinalRow`: the producer's S3 action `min(25%·C_P, 3G)` at opening (the row burn is burned vesting, never in a
    reward base).

  | class | Live / Licensed | FinalRow, producer 13k / 130k / 939k |
  |---|---|---|
  | floor | **320.10** | 325.00 / 960.29 / 960.29 |
  | 8k | **369.52** | 325.00 / 1,123.38 / 1,123.38 |
  | 2M | **6,294.38** | 325.00 / 3,250.00 / 13,000.00 (cap) |

  (`v3_numbers_r2.py`.) The draft's `max(⌈reserved/5⌉, …)` term is dropped: on 2M it made the refuted cost 11,948.59
  against a 6,294.38 reward, 1.9× the reward, contrary to D5's "so honest accusers are not deterred" (V3S-06). Now the
  refuted cost equals r times the nominal base and never exceeds the nominal reward. v2 charged 0.0215 MSK on the
  floor.
* **Refuted: held, then refunded or burned (V3S-06).** A refuted session's exposure moves to `refuted_held` and stays
  in the accuser's exposure ledger (A-6). If the claim is convicted before its record retires (a DA default,
  `ExecutorRefuted`, a `PanelFalseValidV2` conviction, or a court conviction), every `refuted_held` entry returns
  to its accuser in the conviction's block. Otherwise it is burned through `slash_seat` when the claim record retires,
  and earns no reward. So an honest filer of the garbage path (≥ 2 refuted sessions before the conviction) nets ≥ 0;
  the draft left it at −190.10 on the floor, −239.52 on 8k and −17,472.80 on 2M (verifier arithmetic, reproduced).
* A confirmed or released session returns its exposure.
* **This is D5's "≥ r·S, capped at `min_collateral`"**, with S the stage's reward base. An honest accuser whose
  producer was merely slow loses 320.10 MSK on the floor. P2-6 (node policy): a seat that was not served accuses
  once its fetch has failed until `bound + window_receipt − 60`, or at once when a licence lands on a claim that did
  not serve it; every such seat opens its own session (no serialization; DA-9).

**DA-7 (default).** Owner A (M3). Tests T32, T66, T81, T39. At step 2, `sweep_da_sessions` handles each session with
`deadline_daa < now` and an unanswered unit:
* The claim's withholding is **confirmed once**, by the first defaulting session in `(deadline_daa, claim, accuser)`
  order. Every other open session on the claim closes and returns its exposure.
* The effect depends on the claim's stage at the default. "Covering signers" are the `Valid` signers with a live lock
  whose recorded mask covers at least one unanswered unit (C7; a V2 `Valid` and the full seat cover every unit; an
  event unit is covered only by full masks unless T73 pins a leaf mapping for it):

  | stage | effect |
  |---|---|
  | live, unlicensed | `void_and_slash(ProducerWithholding)`: S1, the commitment, strike |
  | `ReceiptLicensed` | S1 at the post-licence price (the commitment; `+ E` from uncommitted stake if released), strike; S4 on each covering signer (X7); the claim voids |
  | `Final` with an unmatured row | the row burned (S3) + producer `min(25%·C, 3G)` + strike; S4 on each covering signer; the #8 reversal while the claim record exists |

* It writes a `DaDefault` (= 5) consumed offence with `execution_root = 0` (V-2b, IMPL-3), which forfeits the claim's
  unminted round rights **by claim** and, for a Final claim, reverses it, and returns every `refuted_held` entry (DA-6).
  Each covering signer's S4 is written under the audit's (seat, claim) key for `PanelFalseValidV2` (N9).
* **The reward (R-1, R-3, V3S-03, V3S-12)** goes to the accuser of the earliest defaulted session (smallest
  `(opened_daa, accuser)` among the sessions with an unanswered unit at their deadline). No `ReporterCommitted` is
  accepted on a DA key. The base is the producer's collected debit only; covering signers' S4 is burned.
* **As integrated (IA-9, IA-10, IA-11).** One hook charges every default: `da_default_charge_v1`, the S-4 funnel opened
  on the producer and every covering signer.
  * **Covering signers are full-mask signers only** (`palw_da_unit_covered_by_v1`, M3 deviation 5; X7). The signer
    S4 is dormant while `palw_da_signer_liability_armed_v1` is false, that is until
    `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` goes `true` with P2-7 (IA-14).
  * **The producer's charge is live from the fence**, whatever the flag: S1 or S3, the `DaDefault` record and the
    reward. So the producer's V2 DA responder (duties from `da_sessions`, answered with `MaterialDisclosedV2`) is an
    unconditional ship condition of every build that arms `palw_rcore_plus` (§8.3 item 9; the M3 review's F2).
  * A **FinalRow** default reverses the `Final` and marks the liability row **`ProducerWithholding`**, never
    `CourtFraud`. So kind 3 cannot read a DA default as a proof against an honest full-mask signer; only N9's
    DA-confirmed gate turns a default against a signer (`479cdfa3`, the M3 review's F1).
  * No strike after `Final`: S3 is its tier.
  * The `DaDefault` record carries the producer's nominal tier and the producer's collected debit only. The reward
    opens in the same block (`open_reporter_reward`, basis `DaDefault`).
  * A session on a named unit already answered is refused (`DaUnitAlreadyAnswered`), so an answered accusation cannot
    be replayed to pause a claim.

**DA-8 (windows and caps; C8, C9, V3S-10).** Owner A (M3). Tests T65, T66, T40.
* **Accusable** from `PanelBound` (a panel exists) through every live phase, and at `Final` while the claim record
  exists **and** its row is unmatured and unmoved; and always only if `now + W_disclose ≤ claim.trace_retention_daa`.
* **The post-Final window, one statement (C8).** The claim record retires at `F + claim_retirement_daa` = F + 3,000,
  deferred while any session is open (DL-1). So a new session may open at a Final claim until F + 3,000, and, while
  sessions keep overlapping, until `trace_retention_daa − W_disclose` = acceptance + 4,200, about F + 4,053 on the
  floor's normal path. The draft's two bullets ("≤ F + 3,000" and "acceptance + 4,200") are both this rule.
* **Residual, named (§4.2 #15):** a row that stays unmatured past that window (the second clock can hold it to
  F + 9,000, and a halt longer) cannot be DA-challenged; only execution-proving convictions reach it (J-2 through the
  liability record or the row, T28).
* **Caps:**
  * a seat of the claim's current panel is **exempt from the lifetime cap**; it has at most one open session (by key)
    and at most `PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1` = 4 sessions on the claim over its life, enough for the
    garbage path's rounds within retention (DA-9);
  * non-seat accusers: at most `PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1` = 3 at once and `PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1`
    = 16 over the claim's life, counted in `opened_non_seat_total`;
  * so the concurrent cap is `seat_count + 3` = 8, and the lifetime cap is `16 + 4 × seat_count` = 36 on t12.

  A Sybil flood never locks a seat out (16 refuted non-seat sessions exhaust only the non-seat budget), and each
  Sybil session that is answered costs its opener its exposure unless the claim is convicted.
* A refuted accuser may accuse again (a new session, a new draw) within its budget.
* **Accuser:** Active, at or above the floor, not the producer, with A-6 headroom for `exposure`.
* **Bytes:** each distinct unit is answered once for every session (`answered`), so at most 36 sessions × 4 units per
  claim, each answer ≤ `max_close_bytes` (80 KiB on the frozen bundle): ≤ 11.25 MiB per claim in the worst case,
  paid by the answering side's carrier fees (accepted, §9.1 Q8). The draft's 5 MiB assumed seats inside the lifetime cap.

**DA-9 (what F3 delivers, and what it does not).** Owner A (M3), B (filers, P2-6/P2-8d). Tests T66, T62.
* **X2 (a quorum that serves only itself).** Each honest unserved seat opens its own session at the licence (P2-6).
  Seat sessions pause the L→F deadline, so the offender must put the binding and every drawn unit on chain within
  1,200 DAA, or take S1 (after licence also S4 on each covering signer; after Final the row burns). A disclosed binding
  runs the identity rule (J-5), so a borrowed root or a relabelled run defaults. A disclosed binding with the right job lets any honest seat recompute an
  attempt job from chain data, name the first divergent step leaf in its next session, and convict by
  `StepArithmetic` (J-6). So `q_X2` becomes "one honest seat that replays and files", not v2's single-index lottery.
* **Rounds fit.** From a licence at about acceptance + 26 the seat's 4 sessions open at about +26, +1,226, +2,426 and
  +3,626, all before the last opening at acceptance + 4,200 (retention 5,400).
* End-to-end on the floor this is T66 and T62. On 8k and 2M it is **UNVERIFIED**: locating the divergent leaf needs
  about `log_1024(leaf count)` rounds of sessions, each ≤ 1,200 DAA, inside the 5,400-DAA retention.
* **Not delivered:** a disclosed unit proves only what it opens. An honest filer is still required; Phase 2 files
  automatically (P2-6, P2-8, P2-8d), and DA-6's refund means a correct filer loses nothing.
