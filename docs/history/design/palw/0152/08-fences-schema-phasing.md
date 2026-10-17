> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 3396–3703 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): §6 fences and schema v22, §7 ownership, mandated order and phasing.
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

## 6. Fences, schema v22, and what does not move

**The fences.** Owner A (S). Tests T24, T02b.

| fence | t12 | hashing | refused unless |
|---|---|---|---|
| `Params::palw_offence_attribution` (the audit's, SPEC §3.1; owner A, M1) | `Some(always())` beside the pass-1 arms, so the pass-2 walk zeroes it | Some-only in every writer (the `palw_clock_floor` pattern), `never()` collapse, schedule id and params id written only when `Some` | DAA 0 only; `palw_objective_offence`, `palw_audit_2026_09_23`, `palw_verification_v2`, `palw_economic_safety` armed; (F1) `palw_prefill_draw` at 0 |
| `Params::palw_rcore_plus` | `Some(always())`, installed by name in pass 1 of `palw_t12_arm_every_rule_from_genesis` | Some-only with the `never()` collapse; `fork_id_v1` entry (ADR-0150) | every prerequisite below is armed at or below it, and the bundle's context root is `COMPLETE_V5` |
| `Params::palw_rcore_attributed_charging` (X10) | **not declared at the regenesis** (S-SPEC §1e, post-edit 9): the field and its extras flag land with M6, which arms it later by a public flag day; being hashed Some-only, adding it then moves nothing on any network | as above | `palw_rcore_plus` is armed at or below it |
| `Params::palw_rcore_conservative_classes` (C7's list) | `[2M class id]`; C7 itself is this list **united with** the ≥ 1,000-span window rule (SR-5, post-edit 5) | **Some-only: written into the fingerprint only when non-empty** (IMPL-6), so every other preset's fingerprint is unchanged | non-empty only with `palw_rcore_plus` armed; a subset of `PALW_T12_GENESIS_HELD_ROWS` (S-SPEC §5) |

**Mirrors in the fold's params (IMPL-6, IMPL-5).** `PalwStateParamsV2` is Borsh-encoded inside the bundle that
`palw_ruleset_id_v2` hashes, so a plain new field would move the ruleset id and `consensus_params_id` on every network
that carries a bundle, including the devnet and mainnet fingerprints T24 keeps pinned (STATE:3232-3233 at `e93be0f2`
says exactly this). The **three** new mirrors therefore follow the existing fence-mirror pattern
(`escrow_backed_exposure_from_daa` and its siblings), all set by one `sync_palw_rcore_plus()` setter (S-SPEC §1e,
post-edit 9):
* `#[borsh(skip)] rcore_plus_from_daa: Option<u64>`, from `Params::palw_rcore_plus`;
* `#[borsh(skip)] withdrawal_delay_daa: u64` = `palw_v2_bond_withdrawal_delay_at_v1(bundle, palw_da_court, 0)`: the
  delay **plus the DA lattice** (12,900 on t12), the value the processor uses, not `bundle.bond.withdrawal_delay_daa()`
  (R-2, B-3; post-edit 10(d));
* `#[borsh(skip)] rcore_conservative_classes: Vec<Hash64>`, C7's list, which the fold needs for SR-1's condition 4 and
  the 2M hold (new in S-SPEC; not in v3.1's row 26);
* `validate_palw_v2` refuses a bundle whose mirrors differ from the params (T24 negative cases).

**Signature contexts (IMPL-7).** The reporter-commit context (R-3) and the DA-disclosure-v4 context (DA-4) are new
ML-DSA-87 contexts. They cannot be appended to `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V4`
(`the_committed_context_set_is_derived_not_retyped` pins V4 as V3 plus exactly three entries), so:
* `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V5` = V4 plus those two contexts, and `palw_v2_signature_contexts_root_v5`;
* the bundle validator accepts the V5 root, and the held-context gate (which today requires the root to equal V4
  exactly when `palw_held_context` is genesis-armed, as on t12) accepts V5 as a superset, like `covers_v3`;
* t12's genesis bundle root is V5, and `validate_palw_v2` refuses `palw_rcore_plus` over any other root;
* the derived-set test learns V5 = V4 plus two entries. T24 has the negative case.

**Prerequisites (X15, F20).** `validate_palw_v2` refuses `palw_rcore_plus` unless all of these are armed at or below it:
**`palw_offence_attribution`** (v3.1: R-core's DA answers, vesting-row copies and tiers read F1/F2's records and
verdicts); `palw_admission_independence` (v3.1: the stake draw relies on its registered-before-anchor cut, SW-8);
`palw_audit_2026_09_23` (which also carries `e93be0f2`'s rate room); `palw_economic_safety`; `palw_objective_offence`
(`ExecutorRefuted` rides `ObjectiveOffence`); `palw_panel_economy`; `palw_panel_exposure_floor`;
`palw_unavailable_abstains` (A-4's premise); `palw_clock_floor` and through it `palw_clock_cursor`;
`palw_verification_v2` (Q-1's V3 receipts); the DA court; and **`palw_operator_id_unique`** (post-edits, SW-A3/R3: the
stake draw's weight is one operator's one bond, SW-2, so it needs operator ids unique; armed at t12 genesis).
`palw_settled_anchor_depth` must be `Some`, and `palw_shard_licensing` must be `None` (Q-3; S-SPEC §5).
T24 has a negative case for each. Two are named because they are easy to miss (F20): `palw_economic_safety` is not in
`palw_fences_v1()` (`params.rs:5852` destructures it as `_`), and `palw_panel_exposure_floor` is a struct with
`.activation`. Every prerequisite is already at DAA 0 on t12 at `4064364e` (`t12_arms_every_fence_t11_armed` passes).

**As integrated (IA-1d; `f422df49`): 14 prerequisites**, one refusal each in `Params::validate_palw_rcore_plus_v1`:
`palw_offence_attribution`, `palw_admission_independence`, `palw_audit_2026_09_23`, **`palw_audit_2026_09_11`** (new),
`palw_economic_safety`, `palw_objective_offence`, `palw_panel_economy`, `palw_panel_exposure_floor` (its
`.activation`), `palw_unavailable_abstains`, `palw_clock_floor`, `palw_clock_cursor`, `palw_verification_v2`,
`palw_da_court` and `palw_operator_id_unique`. **Why `palw_audit_2026_09_11`:** SW-8's one state is the acceptance
walk's object-by-object pre-object base, which the walk folds only past that audit's A-1. Below it the walk rehearses
each object through a whole-block transition while the derivation reads another state, and a binding it dropped could
never be retried. The same validator also refuses a `palw_rcore_plus` not at DAA 0 (genesis only); a missing
`palw_settled_anchor_depth`; `palw_shard_licensing` beside it; a C7 list naming a class outside
`PALW_T12_GENESIS_HELD_ROWS`; a context root other than `COMPLETE_V5`; a panel quorum other than
`PALW_PANEL_COLLUDING_QUORUM_V1` (SR-6's backed-subset check reads that constant); and mirrors that differ from the
params.

**The branch base (V3S-09).** The audit session branches from `4064364e` (the merge of `fix/t12-dos-2026-09-24` and
`feat/testnet-12-regenesis`) for M1/M2, and takes `e93be0f2` **and `f8c91f19`** (the tip of `fix/t12-panel-room`)
before the skeleton, because T-2 depends on both. `f8c91f19` already cites this ADR's T-2(b).

**Genesis.** The params fingerprint moves. The genesis card's K = 64 derivation is not re-derived; it is nominal under
the one ledger (T-3a). The t12 premine txid stays genesis-specific (the replay rule, §8.2). **Roots that move once, at
this regenesis (post-edit 4):** model attempt roots (`CoreV1`, J-5) and free-prompt roots with the tools that derive
them (`misaka-palw-derive`), because the FP `output_root` rendered rule is unified; devnet and drill goldens move with
them, so the drill runs the shipping binary.

**Isolation (F20, reworded).** This binary, like every v21 build since the audit fence (`4064364e` included), cannot run
testnet-11: t11 and `main` are at `PALW_STATE_V2_VERSION` 20, the version is hashed first into every state root, and
headers commit that root. v22 changes nothing operationally for t11, and no fence can restore a t11-identical root.
Only fold **behaviour** is fence-gated; record **encodings** move on every network with v22 (the appended fields of
`PalwClaimStateV2`, `PalwSlashableLockV1`, `PalwPanelLiabilityRecordV1` and `PalwConsumedOffenceV1` change their bytes
whatever the fence says), which matters only to a network that runs this binary's state (IMPL-6). **"Byte-identical"
in this ADR means behaviour and ids, not encodings (post-edit 10(b)):** below `palw_rcore_plus` the fold's behaviour,
the fence schedule id and the ruleset id do not move, but v22 moves encodings everywhere, and the **V2 params id
re-pins once on every V2 preset**, because the state version is hashed into `consensus_params_id` (S-SPEC §1, as it
happened for v21). T24 pins the new ids once.
Past this ADR, **kaspad refuses to start with testnet-11 parameters or a testnet-11 datadir**, with a clear message,
instead of failing deep in decode (T80). **The t11 rollback stays on the `main` binary `1f98d3bf`.** It also refuses,
by name, to start a network that arms `palw_rcore_plus` from a build whose `PALW_RCORE_VESTING_ROWS_LANDED_V1` is
`false` (`palw_rcore_build_can_run_v1`, the S re-review's N1; IA-7). Every other network, and a build with the rows,
passes.

**Schema v22: every field, in one table (v3.1: consistent with SPEC §3.8, §4.1, §4.4).** `PALW_STATE_V2_VERSION` goes to
22 (STATE:237). F2 (M1) changes no record layout (SPEC §3.8; the value 3 appears only in t12's `consumed_offences`). M2
bumps 21 → 22 and appends the audit's fields (SPEC §4.1). S appends every other item below into the **same** v22; no
v23 comes before the regenesis. M3 and M4 only write fields that M2 or S declared. T41's golden vectors are cut at M5
and frozen at launch. Everything is appended last, and enum variants never go mid-enum (the 09-10 Borsh renumbering
lesson). v3's `claim_rcore` side record and its `ClaimRcore` delta are gone: F1 already changes `PalwClaimStateV2`, so
R-core's per-claim fields follow `job_identity` on the claim record and ride its existing delta.

**The S spec is this table's source for S (post-edit 9; S-SPEC lines 17–145).** What that fixes:
* **Landing order M1 → M2 → S → M3 → M4, frozen at M5 by T41.** Delta variants, carriage tails and root sections are
  appended in landing order; their final indices are frozen at M5 by T41 and by the discriminant pin at STATE:38735.
  This replaces the interleaved order v3.1's row 25 listed.
* **R-core's per-claim fields nest** in `claim.rcore: PalwClaimRcoreV1 { licence_door, basis_k, escrow_released,
  served_mask, unserved_seen }` (`Clone, Copy, Debug, Default, PartialEq, Eq`, Borsh). Borsh encodes a nested struct as
  its fields in order, so this is byte-identical to rows 2–6 as five flat fields; it keeps the claim literals at one
  line each, and a redraw resets it with one assignment (S-SPEC P1).
* **The new rooted state** (`withholding_strikes`, `reward_pending`, `reporter_commitments`, `reporter_rewards`,
  `reporter_counters`; then B's `vesting` and counters and M3's `da_sessions` and `da_claims`) goes into one root block
  appended after the last existing block, hashed Some-only when any of it is non-empty or non-zero, and into one
  carriage tail with the `has_*_tail` pattern. The consumed DAA and the accused of a pending reward are read from
  `consumed_offences[offence_key]`, not stored twice.
* **Dormant default.** Where `palw_rcore_plus` is `None` (every network but t12), every S field holds its default and
  no S writer runs.

| # | where | field or item | type | written by (rule) | owner / phase | tests |
|---|---|---|---|---|---|---|
| 1 | `PalwClaimStateV2`, after `rights_reserved` | `job_identity` (attempt: the carrying header's `execution_anchor_v3`; FP: `palw_fp_job_pin_v1(commitment)`; 0 = not recorded) | `Hash64` | `apply_attempt`, the FP claim path, only when `offence_attribution_active` (J-1, SPEC §4.1–§4.2) | A / **M2** | T-THREAD, T18g |
| 2 | `PalwClaimStateV2`, after `job_identity`, inside `rcore: PalwClaimRcoreV1` (post-edit 9) | `rcore.licence_door` | `Option<PalwLicenceDoorTagV1>` | licence, upgrade (SR-2, Q-5); reset to `Default` by `redraw_claim` | A / S; B / M4 (upgrade) | T01, T72 |
| 3 | same | `rcore.basis_k` | `u8` (0 = unlicensed) | licence and supplementary sets, by Q-3's `palw_receipt_set_basis_k_v1` verbatim (S-SPEC P8) | **A / S (S-2)**; B / M4 keeps T71, the Q-5 gate and `NotReplayBacked` | T71, T72 |
| 4 | same | `rcore.escrow_released` | `bool` | licence, SR-1b (monotone, SR-4) | A / S | T01, T74 |
| 5 | same | `rcore.served_mask` | `u32` (bit i = seat i of `panels[claim].seats` carried a backed `Valid`; ≤ 32 seats) | licence, the V2 door (S), SR-10's V3 door (M4) (SR-1, SR-1b, SR-10) | A / S; B / M4 | T27, T68, T74 |
| 6 | same | `rcore.unserved_seen` | `bool` (a latch) | a carried `Unavailable` or `Incapable` (S); `Sampled` (M4) (SR-1) | A / S; B / M4 | T68 |
| 7 | `PalwPanelLiabilityRecordV1`, after `settled_at_final` | `job_identity`, `free_prompt`, `trace_root`, `segment_count` | `Hash64`, `bool`, `Hash64`, `u16` | `persist_panel_liability`, only when `offence_attribution_active` (J-1, SPEC §4.1) | A / **M2** | T46f, T18k |
| 8 | `PalwPanelLiabilityRecordV1`, after `segment_count` | `licence_door`, `basis_k`, **`g_res_sompi`**, **`escrowed_reward`** (post-edit 9: the last two are new, so `G = g_res_sompi + escrowed_reward` is computable after the claim record retires, IMPL-15 and U3's `G_fp`) | `Option<PalwLicenceDoorTagV1>` (`None` for a claim voided before any licence, R8), `u8` (0 when none), `u128` (`palw_max_fraud_gain_v1(facts) − escrowed_reward` with the lock's facts), `u64` | `persist_panel_liability`, at Final and at void (X3, R-1, R8). **IA-8:** `claim_g_v1` reads these recorded values wherever the row exists, never the live facts. **Pending (the audit's commit):** `PalwClaimRcoreV1.g_res_sompi`, `G_res` recorded at licence in `claim.rcore`, so the licence-to-Final interval is fixed too | A / S | T28, T22 |
| 9 | `PalwSlashableLockV1` (appended after `settled_at_final`; stays `Copy`) | `attested`, `segments` | `PalwSegmentMaskV2`, `u16` (`palw_segment_count_v2(seat_count)`) | `lock_valid_seat`: V1/V2 receipts the full mask, SR-10's V3 receipts their own mask (L-3, Q-6; DA-7's covering signers) | **A / S (S-3)**; B / M4 (SR-10's masks) | T73 |
| 10 | new rooted map `vesting: BTreeMap<Hash64, PalwVestingRowV1>` | the V-1 struct, which **copies** `job_identity`, `free_prompt`, `trace_root`, `segment_count` and carries `artifact_root`, `licence_door`, `basis_k` (from `claim.rcore`, not recomputed, S-SPEC P11) | struct | `finalize_claim` (V-1, V-2; N8) | **B / S window** (post-edit 9) | T03, T11, T62 |
| 11 | new rooted map `reporter_rewards: BTreeMap<Hash64 /*offence_key*/, PalwPayoutV2>` | — | | `sweep_reward_reveals` at step 2 (R-4); B's 3d moves and deletes | A / S | T39, T75 |
| 12 | new rooted map `reward_pending: BTreeMap<Hash64, PalwPendingRewardV1>` | `{ amount: u64, reveal_until: u64, evidence_id: Hash64, best: Option<PalwRewardWinnerV1 { committed_daa, commitment, reporter, payload }> }` (v3.1 adds `evidence_id`, N12) | struct | conviction, `ReporterRevealed` (R-3, R-4) | A / S | T39, T75 |
| 13 | new rooted map `reporter_commitments: BTreeMap<Hash64, PalwReporterCommitV1>` | `{ reporter: PalwBondKeyV2, committed_daa: u64 }` | struct | `ReporterCommitted` (R-3); pruned after 3,000 DAA | A / S | T39 |
| 14 | new rooted map `da_sessions: BTreeMap<(Hash64, PalwBondKeyV2), PalwDaSessionV1>` | the DA-2 struct | struct | `DefaultAccused(Held)`, answers, sweep (DA-2…DA-7) | A / M3 (declared in S) | T64–T67 |
| 15 | new rooted map `da_claims: BTreeMap<Hash64, PalwDaClaimV1>` | the DA-2 struct | struct | same | A / M3 (declared in S) | T64–T67 |
| 16 | new rooted map `withholding_strikes: BTreeMap<PalwBondKeyV2, Vec<u64>>` | the DAA of the first S1 in each aligned epoch `⌊daa/1000⌋`; **≤ 9 live** (IA-9; v3.1 said 8: a write at `now` keeps every entry with `now − s ≤ 7,500`, which for `now mod 1,000 < 500` spans the 8 epochs before `now`'s; the list is uncapped, and the code's two doc comments that still say 8 are a doc fix, no behaviour change); entries older than 7,500 DAA dropped when the bond's next strike is written | | S1 (X11) | A / S | T35 |
| 17 | new rooted state | `reporter_counters: PalwReporterCountersV1 { awarded_sompi, forgone_sompi }` (S); `vesting_created_sompi`, `vesting_moved_sompi`, `vesting_burned_sompi` (B) | `u128` each | R-4; V-3 | A / S; B / S window | T03, T55, T56 |
| 18 | `PalwConsumedOffenceV1` (appended) | `collected`, `claim_id` | `u64`, `Hash64` | every conviction past `palw_rcore_plus`, including DA defaults and court convictions (R-2, V-2b). R-core's own; SPEC §2 rejects them as unneeded for F1/F2 (rights are forfeited by claim through `PalwExecFinalV1.claim_id`), and R-2 and V-2b need them | A / S | T39, T81 |
| 19 | `PalwOffenceKindV1` (appended) | `PanelFalseValidV2 = 3`, `ExecutorRefuted = 4` (SPEC §3.2, §4.4); `DaDefault = 5`, `CourtConviction = 6` (R-core, IMPL-3, N6) | enum variants | J-3, J-4, DA-7, the court void | A / M1 (3), M2 (4), S (5 and 6 declared; 5 written from M3) | T46a–n, T18b, T81 |
| 20 | `PalwPanelContradictionV1` (appended) | `IdentityMismatch { binding } = 9`, `OutputMismatch { binding, pin: PalwDecodeTokenPinV1 } = 10`, `ForgedOutputTiled { binding, proof: PalwForgedOutputTiledProofV1 } = 11`, `LogitsNotStepOutput { event: PalwTraceEventDisclosureV1, row: u32, head_tile: u32, head_opening: PalwStepOpeningV1 } = 12` (the hash form, ADDENDUM §4-bis.6; the preimage form is superseded), **`PromptNotAnchored { binding, proof: PalwPromptProofV1 } = 13`** (post-edit 1) (SPEC §4.4, ADDENDUM §4-bis.3–§4-bis.6; sub-enums use `use_discriminant`) | enum variants | J-5 | A / M2 (11 is F1-M, 12 is F1c, 13 is 2M's prompt root; all in the gate) | T18b, T18d, T18e, T18m, T18c(iv), T18u |
| 21 | `PalwReceiptVerdictV2`; `PalwSeatAnswerV2` | `Sampled` (Borsh index 3; message tag 4); `PalwSeatAnswerV2::Sampled` appended (IMPL-11) | enum variants | Q-1 | B / M4 (declared in S) | T70 |
| 22 | `PalwVoidReasonV2` (implicit discriminants) | `UnavailableQuorum` = 5, `NotReplayBacked` = 6 (second panel only) | enum variants | SR-9; Q-5 | A / S (both declared; 5 written); B / M4 (6 written) | T57, T72, T72b |
| 23 | new enum `PalwLicenceDoorTagV1` (explicit discriminants, as `PalwPanelContradictionV1`) | `Quorum = 0`, `Coverage = 1`, `Optimistic = 2`, `ShardPart { quorum_per_shard: u16 } = 3`; `From<PalwLicenceDoorV1>` | enum | SR-2 | A / S | T01 |
| 24 | `PalwConsensusObjectV2` (last today `OptimisticLicensed` = 52) | tag 53 `ReporterCommitted`, tag 54 `ReporterRevealed`, tag 55 `MaterialDisclosedV2` (declared by S-0 with DA-4's shape so that 56 keeps its number; the fold refuses it and the rehearsal drops it until M3, which may refine its payload types before M5), tag 56 `PanelUnavailableQuorum`; **no new tag** for SR-10 (the existing `ReceiptLicensedV2` accepted on a licensed claim), for kind 3 or for kind 4 (both ride `ObjectiveOffence`, tag 51) | object variants | R-3, DA-4, SR-9, SR-10 | A / S (53, 54, 56; 55 declared), M3 (55 written); **B / M4 (SR-10, post-edit 13)** | T39, T64, T57, T74 |
| 25 | `PalwDeltaEntryV2` (after `AnchorDaaPruned` = 65, pinned at STATE:38735) | from 66, **in landing order** (post-edit 9): S's `Strikes`, `RewardPending`, `ReporterCommit`, `ReporterReward`, `ReporterCounters`; then B's `Vesting`, `VestingNote` (apply/revert no-op) and the vesting counters; then M3's `DaSession`, `DaClaim`. No `ClaimRcore` variant: `rcore` rides the existing `Claim` entry | delta variants | V-3, R-4, DA-2 | A / S; B / S window; A / M3 | T40, T48, T41 |
| 26 | `PalwStateParamsV2` | `#[borsh(skip)] rcore_plus_from_daa`, `#[borsh(skip)] withdrawal_delay_daa` (= `palw_v2_bond_withdrawal_delay_at_v1(bundle, palw_da_court, 0)`, the DA lattice included, post-edit 10(d)), `#[borsh(skip)] rcore_conservative_classes` (C7's list; new) (IMPL-6, IMPL-5) | `Option<u64>`, `u64`, `Vec<Hash64>` | one setter, `sync_palw_rcore_plus()`, at bundle build (SR-3, R-2, X16, SW-1, SR-1 cond. 4) | A / S | T24, T40 |
| 27 | `PalwTransitionExtrasV1` (not Borsh) | `offence_attribution_active` (SPEC §3.1); `own_job_anchor` (SPEC §4.2). (`rcore_attributed_charging_active` lands with M6, not S; post-edit 9) | `bool`, `Hash64` | the fences; the processor | A / M1, M2 | T46a, T-THREAD |
| 28 | processor → fold (not state) | `palw_execution_anchor_v1(header, attempt)`; `PalwMergedWorkV1.job_anchor`; `PalwAttemptOriginV1.job_anchor`; `FreePromptCommitted.job_pin` (built in the node, never carried or rooted) | `Hash64` | J-1 (SPEC §4.2) | A / M2 | T-THREAD |
| 29 | derived indexes (never hashed; delta-maintained, rebuilt on apply, revert and `into_state`) | S: `PalwInflightTallyV1.unlicensed_by_bond` (per-class in-flight cache; T-2(a)'s per-bond share), `courts_by_challenger` (A-6), `commitments_by_reporter` and `commitments_by_daa` (R-3's cap and prune); B: rows by `(expiry_daa, claim_id)`, rows by payee; M3: DA sessions by deadline; reveal windows by deadline. **Dropped (post-edit 9, S-SPEC P2):** duties by seat bond, since duties stay in `reserved_exposure` (A-1) | | V-3, A-1, A-6, R-3, T-2(a) | A / S; B / S window; A / M3 | T40, T49 |
| 30 | constants | `PALW_DA_DRAWN_UNITS_V1` = 3; `PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1` = 3; `PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1` = 16 (non-seats); `PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1` = 4; `PALW_RCORE_FINAL_BASIS_K_V1` = 2; `PALW_V2_VESTING_LEGS_PER_BLOCK` = 8 (new keys); `PALW_REPORTER_OPEN_COMMITMENTS_PER_BOND_V1` = 64; `PALW_RCORE_C7_WINDOW_SPANS_V1` = 1,000; S-SPEC's `PALW_RCORE_ACTION_MULTIPLE_V1` = 3, `PALW_RCORE_S1S2_ACTION_PERMILLE_V1` = 100, `PALW_RCORE_S3S4_ACTION_PERMILLE_V1` = 250, `PALW_RCORE_STRIKE_EPOCH_DAA_V1` = 1,000, `PALW_RCORE_STRIKE_WINDOW_DAA_V1` = 7,500, `PALW_RCORE_STRIKE_THRESHOLD_V1` = 3, `PALW_RCORE_REPORTER_REWARD_BPS_V1` = 1,000 (T24 asserts it equals `DnsParams.slashing_reporter_reward_bps` on t12); the audit's `PALW_PANEL_FALSE_VALID_VERSION_V2` = 2, `PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES`, `PALW_J5_INLINE_PROMPT_IDS_V1` = 4,096 and `PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1` = 2^18 (post-edit 1); `PALW_DRAW_WEIGHT_CAP_MSK_V1` = 1,000,000 and `PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1` = 875 (P7, committed at `f1dfb33b`; they replace the draft's `PALW_DRAW_WEIGHT_MAX_MSK_V1` = 2^40) | | | A / M1, M2, S, M3; B / M4 (P7) | T22, T24, T85, T94 |
| 31 | domains and contexts | `PALW_DA_DRAW_DOMAIN_V1`, `PALW_DA_OFFENCE_KEY_DOMAIN_V1`, `PALW_COURT_OFFENCE_KEY_DOMAIN_V1`, `PALW_REPORTER_COMMIT_DOMAIN_V1` and its ML-DSA-87 context, the DA disclosure v4 message domain and context (all in `PALW_STATE_V2_ALL_DOMAINS`; the two contexts form `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V5`, IMPL-7; S-0 needs the byte strings of M3's two); `DOMAIN_VESTING_PAYOUT` and `DOMAIN_REPORTER_PAYOUT` (kept **out** of `PALW_STATE_V2_ALL_DOMAINS`, like the market and refund row-key domains); the audit's key domains `misaka-palw/false-valid-key/v2` and `misaka-palw/executor-refuted-key/v1`; the **two** stake-draw ticket domains (SW-3, in `PALW_PANEL_V2_ALL_DOMAINS`; hashing domains, not contexts; the draft's jury domain is dropped) | | | A / M1, M2, S, M3; B / M4 | T24, T41, T85 |

Not state and not a new tag: `PalwPanelFalseValidEvidenceV2` and `PalwFalseValidReceiptV1` (the audit's, SPEC §3.2,
M1), and kind 4's evidence (SPEC §4.4, M2), all bytes of an `ObjectiveOffence`, with exported constructors returning
`(evidence_id, bytes)` for the Phase 2 filers (C-5); `PalwOffenceTargetV1`, `PalwFaultSiteV1` and
`PalwFalseValidFindingV1` with its `PalwForfeitScopeV1` (the adjudicator's values; post-edit 2);
`PalwPanelDrawPolicyV1::stake` and `PalwPanelStakeDrawV1` (SW-1, P7; resolved at the anchor, never stored). v3's `PalwExecutorRefutedEvidenceV1 { .., network_domain, .. }` and its M4 `PalwPanelFalseValidEvidenceV2 {
valid_receipt, mask }` are withdrawn.

**Unchanged:** `anchor_delay`; the challenge window, `window_court`, `W_disclose` and permit maturity; the fork-choice
order; the ADR-0091 timing; the FP reservation and its abandon hold; Decision A; the rate room of `e93be0f2` (T-2(c)
adds to it). The hold of `f8c91f19` is **re-keyed on C7** by post-edit 5, on the launch line (T-2(b)); nothing
changes below `palw_audit_2026_09_23`.

---

## 7. Ownership, the mandated order, and phasing

### 7.1 The mandated order

| step | what | owner | done when (GREEN) |
|---|---|---|---|
| **M0** | **SEAT-0** (post-edit 4; ADDENDUM §4-bis.10): the seat-only material check (`verify_binding_v1`, J6, J7, the greedy decode token and the dense head relation in `base0_material_tail_matches_v1`), which closes the free execution-root grind on live t12. No consensus change. Drilled on the shipping binary, then patched **live**: the fingerprint must stay `c746f07c`, hosts roll one at a time, and the operator confirms the window first. It also goes into the launch line | A (B deploys) | SEAT-0's T1–T5 regressions give `Mismatch` on the floor, A16 dense and A16 fold; the drill; `c746f07c` unchanged |
| **Launch line** | Fixes that ride the launch candidate, outside the M-order: the licence-stall fix (`fix/t12-licence-stall`); the panel-room hold re-keyed on C7 (post-edit 5; `fix/t12-panel-room`, WIP `wip/t12-c7-held-restore-20260924`); the shard court (post-edit 7, `8be0f661`, J-8, never live); SEAT-0 | B: the panel-room re-key on C7 (finishing `wip/t12-c7-held-restore-20260924`, 5bcc52d7, then fast-forwarding `fix/t12-panel-room`) and the receipt-pool flush (`wip/t12-receipt-pool-flush-20260924`, then fast-forwarding `fix/t12-licence-stall`), HANDOFF §4 step 2; A: SEAT-0 and the shard court, and the review and battery of the candidate | T20, T21 (the C7 cases), the licence-stall pure-function test, the audit's shard-court tests |
| **M1** | **F2 = SPEC §3** (J-3): the fence `palw_offence_attribution`; `PanelFalseValidV2 = 3` with `PalwPanelFalseValidEvidenceV2` / `PalwFalseValidReceiptV1`; the one adjudicator `palw_check_panel_false_valid_v2` in the processor (with the signature) and the fold (without); binding by root; segment-mask liability; one ledger key per (seat, claim); kind 1 and the four named contradictions refused past the fence; the pre-Final void and the post-Final reversal. No record layout changes. **Amended (post-edit 2, landing with M2):** the finding's `acts_on_claim` + `forfeit`, and `ClaimUnderSession` on an open court only. **Ships only in a binary that also carries SEAT-R** (post-edit 3, B) | A | T46a–T46n through the processor gate **and** the fold on producer-built claims (`consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs`); `palw_offence_attribution_is_t12_only`, `palw_offence_attribution_t11_verdicts`; SPEC §3.7's suites unchanged |
| **M2** | **F1 = SPEC §4, with F1-M and F1c (v3.1 decisions 1–2) and the addendum (§4-bis):** v22 bump and `job_identity` threading (J-1); identity checks J1–J7 (J-5), extended to model-class attempts through `CoreV1`; contradictions 9–13 (13 = `PromptNotAnchored` with the heavy budget, post-edit 1); `ExecutorRefuted = 4` (J-4); forfeiture by claim for 9, 10 and 13; the fenced admission rule (canonical job `(n_ctx/8 − 1, 2)`, the head predicate, no Kimi kernel; post-edit 4); the unified FP `output_root` rule (post-edit 4); the court door (J-8); the DA companions (SPEC §4.6) | A | T18, T18b, T18c(i–iii) (under ADR-0111 D3 until M3), **T18c(iv)** (F1c), T18d, **T18e on floor, 8k and 2M** (F1-M), T18f, T18g, T18h, T18k, **T18m** (F1-M), T18p, T18q–T18y, T-THREAD, T62, the Tier B golden, `f1_job_identity_survives_reorg_across_admission`, the v22 goldens |
| **Seat fixes** | **SEAT-S1…SEAT-S4 and `PalwDrillFaultV1`** (ADDENDUM §4-bis.10, non-consensus; the addendum calls the four fixes S-1…S-4, renamed here because S-0…S-8 are S-SPEC's commits). SEAT-S1: the A16 and Qwen3.6 fold branches require whole-context equality with `job_for_anchor` when `attempt_draw` is `Some`, in one shared helper. SEAT-S2: `PalwClaimRootsV1` / `PalwReplayRootsV1` gain `output_root: Option<Hash64>`, filled by the three `execute_for_verdict` implementations and compared in `replay_licenses_v1`. SEAT-S3: an S3 sampler uses a pooled capture only after `verify_material(..) == Matches` (**inside SEAT-R, B**, Q-7). SEAT-S4: a segment opening is authenticated against a verified binding whose root is the claim's execution root, and its committed leaf hashes are checked against `binding.step_merkle_root` before the replay compares (exact shape UNVERIFIED in the addendum). `PalwDrillFaultV1` is the drill hook in `core/palw_backend.rs`, never on a value-carrying network, threaded through the floor, A16, Qwen3.6 and free-prompt loops. They land after M2's admission rule and court door (the addendum's order, step 8) and ship in the binary that arms F2's fence, beside SEAT-R | A (SEAT-S1, SEAT-S2, SEAT-S4, `PalwDrillFaultV1`); B (SEAT-S3 in SEAT-R) | T18p-M (which needs `PalwDrillFaultV1`), the addendum's Tier B `seat_material_duty.rs` and the `CoreV1` golden's same-root two-file seats; they decide the `AnyValid` sites of 9 and 13 (Q-6) |
| **S** | The Phase 1 skeleton, on top of `e93be0f2` + `f8c91f19` (at `0533e1de`, with the C7 re-key): B-1…B-5 (with U2's producer floor gate, S-1 symbols, S-3 wiring); SR-1…SR-9 (the abandon-hold row, S0′ on the second failed panel, SR-1 cond. 4 for C7 (U1), SR-1b in the V2 door, SR-9; **SR-10 moved to M4**, post-edit 13); V-1…V-8 (rows, step 3d, the latch, the halt, A-KEY, the new-key budget, S2 not ticking; **the row machinery is B's, in S's window**, post-edit 9); L-1…L-4 (with the fields for Q-4/Q-6; locks follow the row); A-1…A-6 (the accuser ledger); §3.6 (m = 3, G per claim, the Eq cap, U3's FP producer tier in S-4, R-1…R-7, objects 53/54, `CourtConviction`); H-1's fold half (C-7); T-2(c); DL-1 (non-DA rows); the fences, mirrors, V5 contexts and prerequisites; the rest of the v22 layout (§6; the vesting row's copies, N8; `evidence_id` in `reward_pending`, N12; `DaDefault = 5` and `CourtConviction = 6` declared); `processor.rs` call-site plumbing only (`PalwPanelDrawPolicyV1::stake` is already in place with `None`: P7, `f1dfb33b`, B); the eligibility filter in `PalwPanelValidLockV1` (S-SPEC P7, before M4) | A (B: vesting rows, 3d) | T01–T44 (P1 parts), T02c, T17 (U2), T22 (U3), T47, T57, T74 (V2 door), T75, T76, T77, T78, T80, T81, T82, T84 |
| **M3** | F3: DA-1…DA-9 (drawn units in the run, seat-only pause, re-key, refund, per-seat budgets, `DaDefault = 5` written, N9's shared key for covering signers), N13 (open DA sessions never block a conviction, which closes them), DL-1's DA rows, `MaterialDisclosedV2` with the identity rule, the admissibility helper (C-8) | A | T64–T69, T32, T34 (P1 half), T66's 3-round case |
| **M4** | **Mine (B).** F4: Q-1…Q-7 (the `Sampled` answer, the per-segment recount, the S2 upgrade and first-panel redraw, the lock masks; the evidence type is now M1's), fold half reviewed by A, and the kaspad samplers and collector; **SR-10**, the V3 supplementary door (post-edit 13); **SEAT-R** (post-edit 3), shipped in the same binary as M1's fence. **The stake-weighted draw, SW-1…SW-10:** the key race in `palw_panel_v2.rs` for the class seats and the outsider (the admission jury stays ADR-0147's), the one state and bind-only-in-the-anchor-block rule, the eligible-stake floor, the room's `ready_eff` (T-2(a) amendment, reviewed by A); `palw_panel_draw_policy_at` sets `Some(PalwPanelStakeDrawV1::V1)` | B | T70–T74, T72b, T45, T54e, T18p-M; **T06 on the stake draw, T85–T94** |
| **M5** | Fold / reorg / restart tests on the merged result | A + B | T07 (incl. the reporter twin), T40 (mid-session, mid-gate, mid-hold), T41, T48, T49, T75's reorg twin, T83, T01's revert and IBD twins |
| **P6** | kaspad's producer pre-check (`palw_panel.rs:2438`) reads `palw_producer_facts_v4(.., raw_depth).committed` and the U2 floor gate (post-edit 11). **A hard precondition for any drill of an S-bearing build.** **Integrated (IA-15; `42ef9642`, `0613580f`):** one verdict, `ready_to_produce_v3`, for the node and `getPalwProducerFacts` (wire v9); the FP price answers `ProducerBelowFloor` first; the SW-10 read for the node's stake question is pending | B | T08 (fold = processor = node), T17's node half; `ready_to_produce_v3_reads_the_floor_and_the_committed_ledger_past_the_fence` |
| **P2** | Phase 2 (§7.3) | B | its tests (§8.1) |
| **Gate** | The launch gate and the short drill (§8.3) | B (drill), A (review) | §8.3 |
| **M6** | X10: arm `palw_rcore_attributed_charging` on public t12 | A + B | O-3 shows each failed-attempt strategy attributed live; a drill that crosses the flag-day height |

M1 and M2 go on a new branch off `4064364e`; the skeleton takes `e93be0f2` and `f8c91f19` first. v22 is bumped once, by
M2, and S appends into it (§6). The stake draw's pure half (the key, the sample, T85–T88 on synthetic states) can be
written on its own branch from the start, because it touches only `palw_panel_v2.rs` (the type is there: P7,
`f1dfb33b` on `feat/t12-rcore-stake-draw`); its integration (the anchor-time policy value, the one state and the
bind-only-in-the-anchor-block rule, the room) lands in M4 after M3. The admission jury is not touched (SW-5).

### 7.2 Sequencing rules

* **One writer per file at a time.** `processor.rs` is edited by M1/M2/S (A), then by M3 (A), and only then by Phase 2's
  PROC commits (P2-1…P2-5, P2-9, P2-10). Phase 2 work outside PROC (RPC plumbing in `rpc/*`, CLI, wallet, drill
  scripts) may start after S merges.
* `kaspad/src/palw_panel.rs` is edited by M4 (with SEAT-R and SEAT-S3), P6 and P2-6/7/8/8b–e, all B, serially; SEAT-0
  (A) lands there first, and the licence-stall fix (`a4dfe903`: a re-bound panel is re-judged) is already in. The seat
  fixes that are A's (SEAT-S1, SEAT-S2, SEAT-S4, `PalwDrillFaultV1`) edit the backends and `core/palw_backend.rs`;
  SEAT-S2 also edits `replay_licenses_v1` in `palw_panel.rs`, and that edit takes its own turn in the file's serial
  order, never concurrently with a B commit there.
* **SEAT-R ships in the same binary as F2's fence (post-edit 3, a hard ship rule).** No build that arms
  `palw_offence_attribution` ships, or is drilled as a launch candidate, without SEAT-R. The integration amendments
  extend this to a checklist (§8.3 "Ship conditions", IA-14).
* **P6 before any drill of an S-bearing build (post-edit 11).** Otherwise ADM refuses the attempts kaspad mines.
* The draw functions of `palw_panel_v2.rs` (`derive_panel_v2_with_policy`, the operator entries, the outsider) are
  edited by M4 (B) only; the jury is not edited (SW-5). One exception (S-SPEC P7): S edits only the eligibility filter
  (`PalwPanelValidLockV1::admits` and the draw's headroom test) and lands it before M4 starts. M1 and M2 read the
  receipt functions in that file; if either must edit it, it lands first.
* SW's STATE edits (`panel_rate_v1` / `panel_room_v1` and the ready count) go in after M3's STATE commits, serially,
  and A reviews them because they touch T-2.
* The worktree is never shared while either session commits. No `git stash` across worktrees.
* Every commit runs the kaspa-consensus battery twice (default features and `--features evm`) and has a fence-off twin
  where a dormant branch exists.

### 7.3 Phase 2 (B), updated from `phase2-plan.md`

The plan's §1–§6 stand, with these changes:
* **The API contract (§2)** stands, with these changes to the asks:

  | ask | status in v3.1 |
  |---|---|
  | C-1 `DefaultAccused` on every class | DA-1 (M3) |
  | C-2 disclosure by any Valid signer | DA-4 `MaterialDisclosedV2` (M3) |
  | C-3 `PanelFalseValid` verifies V3 receipts | the audit's `PanelFalseValidV2` with `PalwFalseValidReceiptV1::Segmented` (M1, SPEC §3.2) |
  | C-4 `reporter` and its signature on `ObjectiveOffence` | **replaced** by R-3's objects 53/54 (S); nothing is added to `ObjectiveOffence` |
  | C-5 `ExecutorRefuted` and its constructor | J-4 (M2): `ExecutorRefuted = 4`, SPEC §4.4's evidence; constructors for both kinds return `(evidence_id, bytes)` |
  | C-6 A-6 (the free half) | S |
  | C-7 H-1's fold half | S |
  | C-8 the admissibility helper | DA-3 (M3), now per `(claim, accuser)` |
  | C-9 the default index | DA-3: the named unit comes from the seat's `Unavailable` chunk; `palw_da_default_index_v1` exported (M3) |

  A-KEY (`0x00` keys) is adopted in V-7. Step 3d is written in Phase 1 (the plan's option A), by B inside S's window
  (post-edit 9, §3.3).
* **The contract changes (IMPL-10).** Phase 2's plan §2 must be updated in these places before P2-1 starts:
  * I-4 and the planner: `palw_vesting_mint_plan_v1(state, budget_new_keys, market_waiting)`; the budget is counted in
    new queue keys (V-7), and a row's cost depends on the moves before it in the block, so `PalwVestingLegV1::takes_budget`,
    `leg_count() ≤ 6` and `palw_vesting_mint_position_v1` are restated in keys;
  * `palw_claim_commitment_v1(params, claim, now_daa)` (three arguments; §2.3 listed two; v3's side-record argument is
    gone in v3.1; post-edit 10(a) confirms it supersedes §2.3);
  * reporter sources are keyed by R-3's `offence_key`, which is now the audit's ledger id (one per (seat, claim) for kind 3,
    one per claim for kind 4), and a commitment binds the evidence's `evidence_id` too (N12); DA defaults and court
    convictions have no commitment and name their accuser or challenger;
  * the V3 supplementary door (SR-10) is a new object shape for the collector and the filer, and it is B's own work in M4
    (post-edit 13);
  * the queue lemma test is **T58**, not T46 (R12), so the audit's T46a–n keep their file.

  These are applied in `phase2-plan.md` (2026-09-24, with the post-edits).
* **The contract as the vesting work shipped it (IA-6; `d3ece0d5`).** `phase2-plan.md` §2.3/§2.4 change in these
  places (the handoff copy, `docs/handoff/t12-rcore-20260924/phase2-plan.md` at `4e9fb4f9`, still shows
  `palw_vesting_mint_plan_v1(state, budget_legs)` and a two-argument commitment in §2.3):
  * **The planner takes three arguments**, `palw_vesting_mint_plan_v1(state, budget_new_keys, market_waiting)`. It
    reads the rows, their latches and the reporter rewards, never the queue; the queue enters only through the two
    numbers. **Step 3d calls it** on the state it plans, after 1b's drain, 3′, 3c and the block's latch. It passes
    `budget_new_keys = 8 − palw_vesting_non_market_rows_waiting_v1` (8 under the queue lemma) and `market_waiting =
    palw_vesting_market_rows_waiting_v1`. A row wider than the budget moves only as the plan's first move at full
    width (a liveness belt; no t12 panel has more than 5 seats).
  * **The RPC and Phase 2's coinbase harness call `palw_vesting_next_block_plan_v1(state, params, next_daa,
    raw_depth)`** on a committed state (§2.4's "what moves next block"), never the planner on a committed state.
    It replays the next block's 1b drain (`palw_vesting_market_rows_waiting_after_drain_v1`) and 3d latch (a row
    counts as latched if `palw_vesting_row_maturity_v1` calls it mature at `next_daa` and `raw_depth`), then plans.
    It is empty below `palw_rcore_plus`. It equals the next fold's plan unless that block itself settles an anchor,
    writes a market row, awards a reporter reward, burns or re-keys a row, or finalizes a claim whose new row the plan
    reaches (`the_next_block_plan_is_the_next_folds_plan_over_a_simulated_chain`).
  * **Budget helpers:** `palw_vesting_budget_v1(budget_new_keys, market_waiting)` = `budget_new_keys − min(2,
    market_waiting)`; `palw_vesting_market_rows_waiting_v1`, `palw_vesting_market_rows_waiting_after_drain_v1` and
    `palw_vesting_non_market_rows_waiting_v1`.
  * **Note tags** appended to `PalwVestingNoteV1` (§2.5): **5 `BuybackAtFinal { claim_id, sompi }`** and **6
    `ReserveCredited { claim_id, sompi }`**, which close V-3's identity from deltas.
  * **Position is counted in keys:** `palw_vesting_mint_position_v1(state, claim_id)` returns (moves ahead, keys ahead),
    replacing §2.3's (rows ahead, legs ahead).
  * **Reporter rewards move in the block the sweep wrote them:** step 2's `sweep_reward_reveals` writes
    `reporter_rewards`, and the same block's 3d moves an award to its A-KEY payout key whenever the budget has room, so
    an award is "in `reporter_rewards` or on its payout key" (S-7's `awarded`).
* **P2-6 (automatic DA; normative node policy):** DA-6's cost is 320.10 MSK per refuted session on the floor, held and
  refunded if the claim is convicted. A seat whose fetch has failed files `Unavailable` and a `DefaultAccused` at
  `bound + window_receipt − 60`, and at once when a licence lands on a claim that did not serve it. Every such seat
  files its own session (no serialization; DA-9). No commitment on DA keys (R-3). When 3 `Unavailable` receipts of one
  panel are on hand, file `PanelUnavailableQuorum` (SR-9). On floor and 8k, full-replay a stuck S2 claim and sign a V2
  `Valid` (Q-7).
* **P2-7 (automatic disclosure; a normative duty of a locked signer, V3S-12):** through `MaterialDisclosedV2`; retain
  captures for as long as the lock is live, including the extension DA-5 gives it.
* **P2-8 (capture-arm `ExecutorRefuted`):** build SPEC §4.4's evidence (`ExecutorRefuted = 4`; contradictions 5, 6, 8, 9,
  10, 11, 12, 13; `reporter_reveal` empty) and file through commit (over the ledger key and the `evidence_id`) → wait
  for acceptance → file → reveal. A `PromptNotAnchored` `Whole` filing waits for the block's heavy budget (J-5).
* **P6 (post-edit 11; B; before any drill of an S-bearing build):** kaspad's producer pre-check (`palw_panel.rs:2438`)
  uses `palw_producer_facts_v4(.., raw_depth).committed` and `palw_bond_meets_producer_floor_v1` (U2), and logs
  "holding: top up <shortfall> sompi to reach the producer floor" from `palw_bond_producer_floor_shortfall_v1`. Until
  §9.3 Q11 is decided the code has no in-place top-up, so the help text says that "top up" means registering a new
  bond under a new key and operator identity (B-4). **Integrated (IA-15):** one readiness verdict for the node and the
  RPC (`ready_to_produce_v3`); `getPalwProducerFacts` wire v9 (`bond_committed`, `bond_producer_floor_shortfall`,
  `bond_accuser_exposure`); the FP price refuses below the floor first; the CLI's `E-BOND-BELOW-PRODUCER-FLOOR`.
  **Pending:** a consensus read of SW-10 keyed by `(class, executor bond, candidate DAA)` for the producer's stake
  question (`palw_class_eligible_stake_at_floor_v1`, which answers "unknown" until then).
* **P2-7 and the `AttnFused` design (post-edit 8):** if the audit chooses (B), K/V rows as committed outputs plus
  held-DA, P2-7's retention duty grows to cover those rows; the estimate below does not include it.
* **P2-8b (the replay-mismatch contradiction builder) is now required**, not optional: it is what charges the garbage
  strategy after X10 (§3.9).
* **New:** P2-8c, the kaspad evidence filer agreed with the audit (N10): automatic `PanelFalseValidV2` evidence (`Full` or
  `Segmented` receipts; a located fault only against signers the audit's liability rule makes liable; T54d); P2-8d
  `StepLeaf` demands at the divergent leaf a seat's own replay finds, as a DA session's named unit (J-6, DA-3; T54f);
  P2-8e held dissections for fused-attention leaves (F5, DA-3; T54g).
* **New:** P2-13, the adversarial producer used by O-3 (naive, garbage and borrowed modes). It runs only on public t12
  under an operator **test** bond, never a card key.
* **T10 is no longer Phase 2's pre-launch step.** P2-12 builds the short drill (§8.3) and the observation analyzer
  (§8.4).
* **Effort (estimate, not measured).** The plan's ≈ 22.5 core days, plus about 4 for P2-8b (now required), about 4–6
  for P2-8c/d/e, about 4–5 for M4's F4 half (the per-segment recount, the S2 redraw, the lock masks; the evidence type
  moved to M1), **about 4–5 for the stake-weighted draw** (the integer log and its golden vectors, the key race in two
  draws, the one state and bind-only rule, the eligible-stake floor, the room's `ready_eff`, T85–T94 and T06 on the
  real draw), about 1 for commit–reveal filing and about 1 for the contract changes above. SR-10 (now M4), SEAT-R and P6
  are not in that figure and are not estimated here. The 9-day soak wall time moves after launch.

---
