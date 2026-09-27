> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 1487–1998 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): §3.6 S (slash schedule, reporter reward R), §3.7 T (throughput caps), §3.8 W (withholding), §3.9 P0-10 per attacker strategy.
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

### 3.6 S — the action-based slash schedule (t12), and the reporter reward

Owner A (S; DA rows in M3; `ExecutorRefuted` rows in M2). Tests T02, T02b, T22, T36, T39, T76.

`C` is the accused bond's collateral at conviction. **m = 3** (D4): every action tier is capped at `3 × G`. Every
slash goes through `slash_bond`. What is not paid as reward (R) is burned.

**G, as a function (IMPL-15).** For a claim-bound tier, `G` is the **claim's own** `escrowed_reward + w + rights + s`,
read from the claim record, else the liability record, else the vesting row (J-2): `w` is the claim's commitment
weight, `rights` is `claim_realizable_rights_v1(claim, ..)` at admission (`rr`), `s` is the buyback bound. No class-level
G is used for a claim. For Eq, which binds no claim, `G_eq = palw_eq_cap_basis_v1(state, params, class_id, daa)`:
`claim_escrow_reservation_v1` at the conviction DAA's subsidy, plus the weight reservation and realizable rights a
fresh attempt of `class_id` would reserve at that DAA (the producer pre-check's pricing; the base class if `class_id`
is unknown). The per-class values in §2 are this function at genesis; the class target drifts with retargets, so T76
pins the genesis values and asserts the function, not constants.

**As integrated (IA-3, IA-8).**
* **`s` is in `G_res`.** `G_res = w + R + s` is `palw_max_fraud_gain_v1(facts) − escrowed_reward + s`
  (`rcore_g_res`), with `s` = `rcore_buyback_bound`: the ADR-0091 slice, 5% of `E`
  (`palw_model_buyback_slice_v1`), where the claim's line has a pair open to the buy, else 0. The S review's M2
  reverted an `s = 0` deviation (`0b56c4d8`). The lock prices `s` at its cap (L-1). `G` itself carries the claim's
  own `s`.
* **G is fixed at the fraud.** The tiers read `claim_g_v1`. Where the claim has a liability row it returns the row's
  **recorded** `g_res_sompi` and `escrowed_reward`, never the live facts. The realizable-rights term of the live facts
  shrinks as the claim's rights are realized, and would lower every action priced on G (`min(C₀/4, 3G)`, the Eq cap)
  the longer a conviction waits; on the integration line T66 measured the gap at 1,000,000 sompi. Only a claim with no
  row yet is priced from its own lock facts. `persist_panel_liability` writes the row at the `Final` and at a void
  (§6 row 8), so from licence to `Final` the price is still live. **Pending (the audit's commit):** `G_res` recorded
  **at licence** in `claim.rcore` (`PalwClaimRcoreV1.g_res_sompi`, a new v22 field), which fixes that interval too.
  `basis_k` is the live claim's recount while the claim lives, else the row's.

| tier | action | slashed | vesting | notes |
|---|---|---|---|---|
| **S0** | `BindTimeout`, `NoCapablePanel`, first `ReceiptTimeout`, SR-9 early redraw; after X10 also every S0′ case outside C7 | 0 | — | |
| **S0′** | until X10: second `ReceiptTimeout`, `UnavailableQuorum`, `NotReplayBacked`; for C7 always | the commitment at its stage (`w + esc + rr`) | — | today's #10 (D1, D7); no strike, no action tier, no record, no reward: it opens no conviction (IA-9) |
| **S1** | DA-confirmed `ProducerWithholding` (DA-7) | pre-licence: the commitment; post-licence: the commitment, plus `E` from uncommitted stake if released (X7); post-Final: see S3 | post-Final: whole row | **strike**: one per bond per 1,000-DAA epoch; the 3rd and later strikes within 7,500 DAA add `min(10%·C, 3G)`; the per-claim forfeit always applies. The list holds **up to 9** live entries, not 8 (IA-9): a strike is kept while `now − s ≤ 7,500`, so a write in the first half of an epoch keeps one entry from each of the 8 epochs before it (`palw_rcore_strike_v1`; the list is uncapped, so no behaviour depends on the bound) |
| **S2** | invalid caught before Final: `CourtFraud`, `ExecutorRefuted` (J-4), or a `PanelFalseValidV2` conviction whose finding `acts_on_claim` (contradictions 5, 6, 8–13; post-edit 2: F2's `execution_proving` flag became `acts_on_claim` + `forfeit`) (the audit voids the claim `CourtFraud`, SPEC §3.5 step 7) | the commitment (+ `E` if released) **+ `min(10%·C, 3G)`** | — | once per claim: the void is the marker. **A court default** (`CourtDefault`, void reason 7, written past `palw_offence_attribution` for an unanswered rung or a close declaration that never assembles) is charged exactly as `CourtFraud`: the forfeit plus this action, so silence is never cheaper than losing (operator decision, IA-9). It writes **no** `CourtConviction` record and opens no reward, and kind 3 cannot read it as a proof against signers |
| **S3** | fraud whose claim has an unmatured row: the first `PanelFalseValidV2` or `ExecutorRefuted` conviction binding it (any admitted contradiction), a post-Final DA default, or a post-Final court conviction | producer **`min(25%·C, 3G)`** | whole row | fires once per claim; the row's deletion is the marker |
| **S3-FP** | **U3 (post-edit 12):** an FP claim convicted after Final (it writes no vesting row, so S3 has nothing to burn): the first `PanelFalseValidV2` or `ExecutorRefuted` conviction binding it | executor: a producer action tier **capped at `m × G_fp` = `3 × G_fp`**, with `G_fp` = the liability record's `g_res_sompi + escrowed_reward` (§6 row 8; `escrowed_reward` is 0 on FP). The post-edit fixed the cap, not the rate; **S-4 fixes the rate at S3's: `min(25%·C₀, 3·G_fp)`** (`palw_rcore_s3s4_action_v1`; IA-9). The once-per-claim marker is the liability row's first conviction mark (`post_final_fp_g_v1`) | none (no row) | once per claim, like S3; the signers' S4 and the root forfeiture are unchanged; lands in S-4 (T22) |
| **S4** | false Valid: a `PanelFalseValidV2` conviction of this signer under the audit's liability rule (a `Full` receipt always; a `Segmented` receipt by its mask and the fault's site, SPEC §3.3 step 9); `ProducerWithholding` only when DA-confirmed (N9); or DA non-disclosure by a `Valid` signer whose mask covers an unanswered unit (X7, DA-7), under the same (seat, claim) ledger key | lock (taken first, SPEC §3.5 step 4) + **`min(25%·C, 3G)`** | whole row (via S3) | never escalates; once per (seat, claim); `Sampled` signers are never S4 (Q-1: `Sampled` is not `Valid`, and the adjudicator requires `Valid`). **The action needs a live lock (the audit's #12; IA-9):** it applies only while the seat holds its lock on the claim; a seat the liability row lists that holds none is convicted for 0 |
| **S4′** | `PanelFalseValid{ConflictingPermit}` — **unreachable on t12 (v3.1):** refused by name past `palw_offence_attribution` | lock | the seat's share | below the fence only (X6) |
| **Eq** | `ExecutorEquivocation` (standalone), t12 | **`min(C, 3 · G_eq)`** (D6) | none | `G_eq = palw_eq_cap_basis_v1` of the class whose id equals the certificate's `job_context.shape_profile_id`, else of the base class; no status change |

**The conviction funnel, as S-4 implements it (IA-9; `c3fe99cd`).** Every conviction past `palw_rcore_plus` runs in
three parts; below the fence every path is the old one, byte for byte. **S0′ is not a conviction:** it opens nothing
and writes no record (`void_and_slash_at` takes its forfeit, with no strike, action or reward).
1. **It opens** (`open_conviction_v1`). For every bond it may charge, it records `C₀`, the posted collateral before this
   conviction's first debit, and whether the bond's exit gate was **shut** on the pre-state
   (`palw_bond_collateral_is_locked_v6` at the conviction DAA, with the raw second-clock depth and the mirrored
   withdrawal delay). It does this before any leg runs, in particular before a kind 3 or DA-7's S4 deletes the lock
   that v6 reads.
2. **The legs run** through `slash_bond`, which returns its debit:
   * `void_and_slash_at` covers S1 (the forfeit and a strike, `palw_rcore_strike_v1`) and S2 (the forfeit plus
     `palw_rcore_s1s2_action_v1`, including the court default);
   * `post_final_producer_leg_v1` covers S3 through `burn_vesting_row`, or U3;
   * S4 is the lock plus `palw_rcore_s3s4_action_v1`;
   * Eq is `palw_rcore_eq_cap_v1`.
3. **It closes** (`close_conviction_v1`) with the consumed record written **last**. `amount` is the nominal tier
   summed over the conviction's legs. `collected` is the debits taken from the bonds whose gate was shut: 0 for a
   gate-open bond, the real remainder for a pre-drained one (R-2). `claim_id` is set (0 for Eq).

Per path:
* **Kind 3** (`convict_false_valid_rcore_v1`) opens on the seat **and** the claim's producer. Its record's `amount` and
  `collected` **include the producer's leg** when the finding acts on the claim: the S2 void before `Final`, or S3/U3
  after it (S-4's implementation, `c3fe99cd`; the doc of `convict_false_valid_rcore_v1`).
* **Kind 4** (`consume_executor_refuted_v1`'s funnel branch) opens on the executor alone.
* **A proven court verdict** goes through one entry, `convict_by_court_verdict_v1`. That covers the court's
  `ExecutorGuilty` close, whole or assembled; the shard and checkpoint courts' one-move `ExecutorGuilty`; the held-leaf
  verdicts; and, once A-held is merged, its dissection verdict site. It writes the `CourtConviction` record (kind 6) through
  `record_court_conviction_v1`. **A court default never reaches it** (S2 row above).
* **A DA default** (`da_default_charge_v1`) writes `DaDefault` (kind 5) with the producer's nominal tier and the
  producer's collected debit only. Each covering signer's S4 is its own kind-3 record, burned and never rewarded (DA-7).
* **Eq** (`convict_equivocation_rcore_v1`) records claim 0 and root 0.

Tests: `rcore_s4_conviction_funnel` (S1 strikes, S0′, T76, R-2's gate-open and pre-drained bonds, T81, a court default
with no kind-6 record); the lib's `s4_conviction_funnel`; T46 on real claims.

**Values** (MSK; v3 script):

| | floor | 8k | 2M |
|---|---|---|---|
| S2 action, 13k / 130k / 939k | 1,300 / 9,602.89 / 9,602.89 | 1,300 / 11,233.85 / 11,233.85 | 1,300 / 13,000 / 93,906.32 |
| S3/S4 action, 13k / 130k / 939k | 3,250 / 9,602.89 / 9,602.89 | 3,250 / 11,233.85 / 11,233.85 | 3,250 / 32,500 / 190,797.47 |
| Eq, 13k / 939k | 9,602.89 / 9,602.89 | 11,233.85 / 11,233.85 | 13,000 / 190,797.47 |

The producer term stays at 25% × 13,000 = 3,250 for every class, below `G` for 8k and 2M. D4 chose not to raise
the producer floor; §4.3 shows that safety instead rests on the recounted `basis_k` independent attesters.

**X7, the disclosure rule (kept, extended to F3; C7).** A DA session on a licensed claim, or on a Final claim with an
unmatured row, is answered by the producer or by **any** bond with a live lock on the claim (`MaterialDisclosedV2`,
DA-4). If a session defaults (DA-7), the producer is charged S1 at the stage's price, and S4 is charged only to a
`Valid` signer with a live lock **whose recorded mask covers at least one unanswered unit** (a V2 `Valid` and the
full seat cover every unit; Q-6). **v3.1 (N9, agreed with the audit):** the signer's S4 is written under the audit's
(seat, claim) ledger key `palw_offence_id_v1(PanelFalseValidV2, seat, H("misaka-palw/false-valid-key/v2" ‖ claim_id))`,
so a `ProducerWithholding` contradiction filed later against the same signer is a no-op, and one filed first makes
DA-7's charge a no-op; `ProducerWithholding` is admitted only against a claim whose void is a DA-7 default, never
against a seat that answered `Sampled`, `Incapable` or `Unavailable` (none of them is `Valid`), and never above the S4
tier (m = 3, `3 × G`). Its site is `Whole` (SPEC §3.3), so a `Segmented` receipt is liable only with the full mask. A partial seat holds and attested only its own segment
(`palw_verification_v2.rs:15-19`), so it is never charged for a unit outside it.
* **As integrated (IA-11, M3 deviation 5): only full masks cover.** `palw_da_unit_covered_by_v1` is `segments > 0 &&
  attested.is_full(segments)`, for event and held units alike. Placing a held step leaf in a segment needs the claim's
  committed step-leaf count, which no record the fold keeps carries after the accusation. Binding a partial seat would
  need a new v22 field, and the operator decided partial seats are not bound at launch. So DA-7's signer S4 charges
  **full-mask signers only**: a colluding partial seat is under-charged, never an honest one over-charged. The signer
  half is dormant until `palw_da_signer_liability_armed_v1` holds, that is until `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1`
  is `true` (IA-14); the producer's DA-7 charge, its record and its reward do not wait for it, which is why the
  producer's V2 DA responder ships unconditionally (§8.3 item 9).
* **Residual, named (review L10):** if every honest covering signer is offline for `W_disclose` = 1,200 DAA, each is
  charged S4, a charge for silence. P2-7 makes retention and automatic disclosure a **normative duty** of a locked
  signer (V3S-12), and a DA default pays no reward on signers' debits (R-1).
* **Structural residual, named (C7, §4.2 #16):** partial seats and FP-interval seats hold no capture of units outside
  their segment or interval. When the full seat is the attacker's (or absent) and the producer is silent, nobody
  honest can answer those units; the default then charges the producer and the covering (colluding) signers only.

**Removed and deferred** (as v2): S5(a), S5(b), the vesting burn and ejection on equivocation, tombstones —
until Q1 (runtime nondeterminism) is settled.

#### R — the reporter reward

**R-1 (amount).** Owner A (S). Tests T39, T22. `reward = ⌊ r × max(0, collected − X) ⌋`, `r` = 1,000 bps
(`DnsParams.slashing_reporter_reward_bps`, `params.rs:8595`).
* `X` is the part §4 needs to cover value already extracted: `min(lock, G_res / basis_k)` for each load-bearing
  signer's lock on a claim that reached Final; 0 otherwise.
* Burned vesting is never in the base. RT#2 (S0′) forfeits and refuted-accusation charges pay no reward.
* **A DA default's base is the producer's collected debit only (V3S-12).** Signers' S4 debits on a DA default are
  burned, not rewarded, so hunting silent signers is never a bounty. For a floor coverage Final the v3 draft would
  have paid about 5,206.5 MSK against a 320.10 refuted cost (break-even at 5.8% chance that every discloser is silent,
  verifier arithmetic, not recomputed here).

**R-2 (the base is the collected debit; D5, F10, IMPL-5).** Owner A (S). Test T39.
* `collected` is Σ over this conviction's `slash_bond` calls of `collateral_before − collateral_after`. `slash_bond`
  saturates (STATE:13373), so a pre-drained bond yields its real remainder, not the nominal tier.
* A bond whose withdrawal gate is **open** at the conviction contributes **0** to `collected`: its collateral may
  already have left through the UTXO layer, where no burn can reach it. The slash is still recorded. "Open" is
  exactly `!palw_bond_collateral_is_locked_v6(..)` evaluated by the fold at the conviction DAA (IMPL-5). The fold
  reads the withdrawal delay from a new `#[borsh(skip)] withdrawal_delay_daa` mirror in `PalwStateParamsV2`, set by
  the same setter as the fence mirrors, and `validate_palw_v2` refuses a bundle whose mirror differs from the params.
  **The mirror includes the DA lattice (post-edit 10(d)):** it is
  `palw_v2_bond_withdrawal_delay_at_v1(bundle, palw_da_court, 0)`, 12,900 on t12 (7,500 plus the lattice's 5,400), the
  value the processor uses, not `bundle.bond.withdrawal_delay_daa()` (7,500). "Already released" is dropped as a separate clause: the fold cannot see the UTXO
  spend (a bond record is never removed after release; STATE:7580 at `e93be0f2`), and the predicate above already
  covers every bond that could have left.
* `PalwConsumedOffenceV1` gains `collected: u64` and `claim_id: Hash64` (appended, §6). `amount` keeps the nominal
  tier for audit. Today's record writes the nominal amount after a saturated debit (STATE:~10401-10411 at
  `4064364e`; probe `adr0152v2_r2_saturated_conviction_records_nominal_amount`).

**R-3 (commit–reveal; D5, F7, V3S-03, V3S-11).** Owner A (S). Tests T39, T75. Two appended objects:
* `ReporterCommitted { commitment: Hash64, reporter: PalwBondKeyV2, signature: Vec<u8> }` (tag 53), with
  `commitment = H(PALW_REPORTER_COMMIT_DOMAIN_V1 ‖ offence_key ‖ evidence_id ‖ reporter ‖ salt32)` (v3.1 adds
  `evidence_id`, N12), signed by the reporter bond's
  key over `palw_reporter_commit_message_v1(network, commitment, reporter)` under a new ML-DSA-87 context (§6,
  `COMPLETE_V5`). The reporter is Active and at or above the floor. It is rooted in `reporter_commitments` with its
  `committed_daa`, at most `PALW_REPORTER_OPEN_COMMITMENTS_PER_BOND_V1` = 64 live per bond.
* **Pruning (V3S-11).** A commitment is pruned at step 2 once `now > committed_daa + window_court` (3,000 DAA)
  **and** no open `reward_pending` entry was consumed at a DAA above its `committed_daa`. The commitment hides its
  key, so the fold protects every commitment older than the newest open pending conviction; the derived index by
  `committed_daa` makes this one range cut. So a commitment made at detection is never pruned before a reveal it
  could win.
* `ReporterRevealed { offence_key: Hash64, reporter: PalwBondKeyV2, salt: [u8; 32] }` (tag 54). Anyone may carry
  it. It is accepted iff the commitment recomputed with the **consumed** offence's `evidence_id` (stored in
  `reward_pending`, R-4) exists with `committed_daa` **strictly below** the DAA of the block that consumed the offence,
  and `reporter ≠ accused`.
* **Why `evidence_id` (N12, v3.1).** The audit's ledger keys are one per (seat, claim) for kind 3
  (`palw_offence_id_v1(PanelFalseValidV2, seat, H("misaka-palw/false-valid-key/v2" ‖ claim_id))`) and one per claim for
  kind 4 (`palw_offence_id_v1(ExecutorRefuted, executor, H("misaka-palw/executor-refuted-key/v1" ‖ claim_id))`). Both
  are computable at bind, before any fault is found, so v3's commitment over the key alone would let a speculator commit
  to every (seat, claim) pair and win every reward (V3S-03's hole, reopened by the key change). `evidence_id` is the
  `ObjectiveOffence`'s digest of its evidence bytes, which the fold checks (SPEC §3.5 step 1); with `reporter_reveal`
  empty it is a function of the contradiction that was actually verified, which nobody can predict before detection.
* **The evidence's own slot stays empty on t12.** `PalwPanelFalseValidEvidenceV2.reporter_reveal` and kind 4's
  `reporter_reveal` are empty (`reporter_armed = false`, SPEC §3.3 step 2); R-3 uses objects 53/54 and never that slot,
  which stays reserved and outside every conviction hash.
* **Which keys take commitments.** Only convictions on evidence the filer supplies: a `PanelFalseValidV2` (kind 3) or
  `ExecutorRefuted` (kind 4) conviction whose contradiction is 5, 6, 8, 9, 10, 11, 12 or 13 (post-edit 1); `offence_key` is its ledger id
  above. As integrated, a standalone `ExecutorEquivocation` (kind 0) takes commitments the same way, on its consumed
  `evidence_id` (S-SPEC P10; IA-10). A kind-3 conviction on contradiction `ProducerWithholding` (2) or `CourtFraud` (4) restates a DA default or a
  court verdict, takes no commitment and pays no separate reward (the signer's debit is burned, as V3S-12 already burns
  signers' debits on a DA default). **A DA default and a court conviction take no commitments (V3S-03):** their keys
  (`palw_offence_id_v1(DaDefault, producer, H(PALW_DA_OFFENCE_KEY_DOMAIN_V1 ‖ claim_id))` and
  `palw_offence_id_v1(CourtConviction, producer, H(PALW_COURT_OFFENCE_KEY_DOMAIN_V1 ‖ claim_id))`, V-2b) are public when
  the claim is admitted, so a speculator committing to every admitted claim would beat every honest accuser. A DA default's reward goes to the
  accuser of the **earliest defaulted session** (DA-7); a court conviction's reward to the court's challenger. Their
  records still use those keys (V-2b).
* **The winner** of an execution-proving reward is the matching commitment with the smallest
  `(committed_daa, commitment)`. A copier who learns the evidence from the mempool can commit only after the honest
  reporter did, so it loses. Honest reporters commit, wait for the commitment to be accepted, then broadcast the
  evidence (Phase 2 node policy, P2-8).
* **Not an incentive against a rational offender (V3S-03).** On the garbage path the contradiction is built from bytes
  the offender itself discloses, so the offender's Sybil can commit to it first; one ledger key per (seat, claim) then voids honest
  commitments to other digests. The offender nets `−(1 − r)·(collected − X)`, still a loss (R-7), but **R pays an
  honest filer nothing against an offender that pre-commits**. Precondition (i) (§4.1) therefore does not rest on
  R: filing rests on Phase 2's automatic filers, and DA-6's refund makes a correct filing cost nothing. "reporter ≠
  accused" is hygiene only; B-4 says identity is unverifiable.

**R-4 (timing; F17).** Owner A (S). Tests T75, T07. At conviction the fold writes `reward_pending[offence_key] = {
amount, reveal_until = now + window_receipt (600), evidence_id, best }` (`evidence_id` of the consumed evidence, v3.1
N12; 0 for a DA default or a court conviction). For a DA default or a court conviction `best` is set to the
earliest defaulted session's accuser, or the challenger, and no reveal is accepted. Reveals update `best` at step 3.
At step 2 of the first block past `reveal_until`, `sweep_reward_reveals` writes the winner's reward to
`reporter_rewards[offence_key]` (payload fixed then), or adds the amount to `reporter_forgone_sompi` if there is no
winner. Step 3d moves `reporter_rewards` first within the budget (V-7). A sweep-time conviction (S1 at a DA deadline,
a court backstop) enters `reward_pending` at step 2 the same way. Nothing writes the queue at steps 2 or 3, so the
rehearsal and the pre-object mirror are untouched.
* **Proven-only, as S-7 ships it (IA-10; `1f4b2b2a`, wired at `672436d8`).** Every conviction closes by calling one
  seam, `open_reporter_reward(now_daa, offence_key, extracted, basis)`, after its record is written. The seam opens a
  reward **only for a proven conviction**, and it checks the basis against the consumed record's kind:
  * `CheckedEvidence { evidence_id }` for kinds 0 (Eq), 3 and 4, by commit–reveal on the consumed `evidence_id`.
    Eq's reward is commit–reveal too (S-SPEC P10);
  * `CourtVerdict { challenger }` for kind 6;
  * `DaDefault { accuser }` for kind 5.

  A **court default** (`CourtDefault`) opens nothing, and its debit stays burned. A kind-3 conviction on contradiction
  2 or 4 opens none (it restates a void). A DA default **pays**, to the earliest defaulted session's accuser, on the
  producer's collected debit only. That is a recorded decision in the seam's doc: the offence is the withholding
  itself, of material the producer was bound to keep and disclose (P2-7), not a presumption drawn from an absent
  move. DA-7's charge hook (`da_default_charge_v1`) opens it in the block the `DaDefault` record is written. A zero
  reward opens nothing; a named winner equal to the accused, or one with no bond, is recorded with no winner, so the
  amount is forgone.

**R-5 (invariant with R paid).** Owner A (S). Tests T03, T39. After Final the chain keeps `(1 − r)·ΣS + r·X ≥ G_res` ⇔ `ΣS ≥ G_res`, which holds
because the `basis_k` convicted locks sum to at least `1.1·G_res + 0.1·E_v`. Before Final nothing was extracted, so
`X = 0`. The reward is minted before the UTXO-level burn (the slash burns only when the bond's release spend leaves
`slashed` unclaimed); that is sound only because no fold path ever un-slashes (review L34-7). T03 and T39 count
unreleased `slashed` as burned and assert `Σ R ≤ r · Σ collected`.

**R-6 (the premise j = k; F16).** Owner A (S). Test T39. R-5 holds when all `basis_k` load-bearing locks of the faulty segment are live and
convicted, and `S` is the collected debit. Chain-kept, row plus j locks, net of R (v3 script):

| class | k = 3: j = 3 / 2 / 1 | k = 2: j = 2 / 1 | G_res |
|---|---|---|---|
| floor | 3,489.05 / 3,392.98 / 3,296.91 | 3,489.05 / 3,344.95 | 0.12 |
| 8k | 4,081.63 / 3,788.04 / 3,494.44 | 4,081.63 / 3,641.24 | 543.77 |
| 2M | 69,323.08 / **47,282.34** / **25,241.59** | 69,323.08 / **36,261.96** | 60,398.31 |

The bold 2M cells fail. T39 carries them as **expected-FAIL until ADR-0153**.

**R-7 (no farming).** Owner A (S). Test T39. A coalition that convicts itself nets `G_res − ΣS + r·(ΣS − G_res) = −(1 − r)(ΣS − G_res) < 0`
after Final, and `−(1 − r)·S` before. The v2 review's 2M exited-bond self-report (+7,510 MSK) is closed by R-2: an
exited bond contributes 0.

**Examples** (m = 3, 13,000 MSK producer, 130,000 MSK seats; v3 script):

| conviction | collected | reward | attacker net | chain keeps (after R) |
|---|---|---|---|---|
| DA-confirmed withholding, pre-licence, floor | 3,200.95 | 320.10 (8k 369.52; 2M 6,294.38) | — | — |
| floor post-Final fraud, V1 (k = 3), full conviction | 32,378.89 | 3,237.88 | −35,579.62 | 32,341.86 + row 3,200.85 burned |
| floor post-Final fraud, coverage (k = 2) | 22,776.00 | 2,277.59 | −25,976.73 | 23,699.26 + row |
| 8k post-Final fraud, V1 / coverage | 37,869.78 / 26,635.93 | 3,732.60 / 2,609.22 | −40,526.86 / −29,293.01 | |
| 2M post-Final fraud, V1 / coverage | 167,508.23 / 135,008.23 | 10,710.99 / 7,460.99 | −110,310.76 / −77,810.76 | |
| Eq of a 13k bond (floor class) | 9,602.89 | 960.29 | | |
| Eq of a genesis bond (2M class) | 190,797.47 | 19,079.75 | | |

"Attacker net" is `G_res − (row + collected)`; if the offender captures its own reward, add the reward back
(floor V1: −32,341.74).

### 3.7 T — protocol throughput caps

**T-1 (cadence).** Owner A (S). Test T08. Unchanged: one own attempt per chain block plus merged blues; per-class
epoch budgets and `palw_admission_claims_per_span_v1`; past the grace, the D5 room. The floor is the base class and
is ungated by class caps; its bound is at most about 1 claim per DAA.

**T-2 (panel capacity; the panel-room fix `e93be0f2` and its review `f8c91f19`; D7; V3S-09 as corrected by post-edit
5).** Owner A (S); the launch-line room fix (the C7 re-key below) is B's, reviewed by A (HANDOFF §4 step 2). Tests
T20, T21, T72, T91.

* **(a) The D5 room is `e93be0f2`'s rate rule, made exact by `f8c91f19`**, past `palw_audit_2026_09_23` (t12 only):
  * `room_c = capacity_c − owed_c`, where `palw_panel_capacity_by_rate_v1` is the largest n whose whole Q32 term
    (each class's owed replay charged over its own verification window, rounded up per class) still fits beside the
    other classes' terms (`palw_work_target_v1.rs`; STATE `panel_room_v1` / `panel_rate_v1`). `per_span_c` = ready seats
    × `reference_work_per_span` × utilization. Another class's window never enters a class's room; there is no
    fallback horizon. Fail-closed arithmetic: a term that does not fit is `u128::MAX`, a capacity that does not fit
    is 0.
  * **A licence frees the replay of a class outside C7**, the 8k row included (`PalwInflightTallyV1::replay_pending`;
    post-edit 5). **A court open on a licensed claim charges it again** until the court closes.
  * **Utilization is not a lifecycle input.** The row records the reading; the room refuses the claim the panel
    cannot schedule, and the class is not held for it.
  * **The block's own attempt is judged where step 3 began** (`own_attempt_fit`, `room_exempt_class`); a gated object
    of another class is refused where the own attempt fitted before it and would not after it (`f8c91f19` item 2).
    SR-10 supplementary sets and Q-5 redraws change the owed replay without asking the gate; as with `f8c91f19`'s
    courts, the change takes effect for the next block's own attempt. Past `palw_rcore_plus` a DA session changes no
    phase and charges nothing (T-2(c)), so the accusation re-charge `f8c91f19` names does not arise there.
  * Below the fence the common-horizon rule and the utilization hold stand verbatim (testnet-11).
  * **The gated claim is pooled before it is counted (`0533e1de`, R11).** Past `palw_audit_2026_09_23` the class gate
    takes the claim as `PalwGatedClaimV1` (an attempt, or a free-prompt commitment of so many quanta;
    `check_class_admits_claim`, STATE:9344 at `0533e1de`) and pools it into `palw_panel_owed_v1` before counting
    (`panel_rate_v1`'s `extra: Option<(Hash64, PalwGatedClaimV1)>`, STATE:9523), so a one-quantum commitment that fills
    its class's last part-job raises no term and is admitted; an attempt adds one whole job, as before. A C7 class's
    static cap still counts the claim asked about whole. T21 asserts both against `0533e1de`.
  * This supersedes v2's T-2(a) formula `c = max(1, ⌊S × W × ρ × 0.7 / (V × 5)⌋)`.
  * **v3.1 amendment (SW-9):** past `palw_rcore_plus` the `ready` factor of `per_span_c` is `ready_eff`, the effective
    number of ready operators under the stake draw over SW-2's capped weights, not the count of ready bonds
    (`model_registry_ready_seats`, STATE:9551 at `0533e1de`). At genesis nothing moves: the floor is ungated, 2M is
    capped (C7), and the 8k row's `ready_eff` is 8, its ready count (eight genesis operators). **After post-edit 5 the
    8k row is room-governed**, so `ready_eff` applies to it as soon as operators other than the genesis seats are ready
    (SW-9, T91).
  * **Per-bond share (capital pricing, not identity):** no bond holds more than `⌈c_class/2⌉` unlicensed claims of a
    non-base class, where `c_class` is `palw_panel_capacity_by_rate_v1(per_span, 0, window, cost)` for a class outside
    C7 and `max_inflight_claims` for a C7 class. It reads the derived index of unlicensed claims by `(bond, class)`
    (IMPL-14).
* **(b) C7 is held to Final and to its static cap (`f8c91f19`'s hold, re-keyed on C7 by post-edit 5; D7).** Past
  `palw_audit_2026_09_23`, `palw_panel_owed_v1` makes a C7 class owe **every** claim in flight until Final: no licence
  releases it and a court adds nothing to it. `check_class_admits_claim` refuses a C7 class past
  `row.profile.max_inflight_claims` (`ClassInflightCapped`) before the room is read, and op 186's `panelRoom` for a C7
  class is `min(room, cap − owed)`.
  * **The predicate (post-edit 5, correcting V3S-09).** `f8c91f19` keyed six places on `class_is_held_v1` (membership
    of `class_step_ladders`): the static cap, the room minimum in the gate and in op 186, owing every claim until
    Final, the court add-back skip, and the walked oracle. Both t12 genesis model rows register a held map, so that also
    held the 8k row (the room re-review's N1: after an 8k licence the room and gate still read `(0,
    ClassInflightCapped(5/5))`, and 8k throughput fell by about 5.7× at 8 ready seats and about 9× at 12). The fix keys
    all six on **C7** (SR-5): `verification_window_spans ≥ PALW_RCORE_C7_WINDOW_SPANS_V1` (1,000), united with
    `palw_rcore_conservative_classes` once R-core+ lands. At t12 genesis that is exactly the 2M row (window 2,799; the
    8k row's window is 3). It restores the user-approved v2 T-2(a)/(b), and the window rule also catches a later heavy
    class that registers no held map (the re-review's N4). The code fix is on `fix/t12-panel-room` (workflow
    wxt13xc23; WIP `wip/t12-c7-held-restore-20260924`, with `panel_room_short_class_is_released_at_licence`, the two
    HELD-2 cases and t11 parity; B finishes it and fast-forwards the branch, A reviews).
  * 2M: `c_2M = 1` until ADR-0153 and a measured 2M replay. v2's reasons stand: at 8 ready seats two overlapping
    panels share ≥ 2 seats, and a reference-speed replay (≈ 9.7 d) does not fit the 2,799-DAA window. **Past the
    integration amendments 2M is closed at launch (U-D1, §3.9)**; this hold and cap bound it only until 4-quater's
    closure lands, and after a flag day opens it.
  * **8k is released at licence**, like every class outside C7: an 8k licence frees its replay (T-2(a)), and its static
    cap (`max_inflight_claims` 5 on the t12 row, `t12_regenesis.rs:181-184` per the room re-review) is not consulted
    past the grace. O-2 reports 8k throughput live.
  * The held-context rows (`PALW_T12_GENESIS_HELD_ROWS`, `params.rs:11128` at `f8c91f19`: 8k and 2M) keep what a held
    context means for DA and courts (held units, DA-3; J-8), not a panel-room hold.
* **(c) An S2 licence keeps its replay charged (IMPL-9).** Past `palw_rcore_plus`, `palw_inflight_index_note_v1`
  counts a claim as licensed only if it is `ReceiptLicensed` **and** `claim.rcore.basis_k ≥ 2`. The note reads the
  claim record (v3.1: `basis_k` is nested in `PalwClaimStateV2` as `claim.rcore.basis_k`, not a side record), and every
  write of `basis_k` removes and re-adds the claim's contribution in SR-3's funnel, so an S2 licence stays charged and
  an upgrade frees the replay in the block it lands.
  `palw_inflight_index_build_v1` and op 186 read the same pair. This matters for non-base classes outside C7, the 8k
  row included (C7 owes until Final anyway; the floor is ungated). A DA session does not charge the room: it is disclosure,
  not panel replay.
* **(d) Licence capacity of one bond (F12).** For a 13,000 MSK producer (two concurrent unlicensed floor claims), with
  `f` the share of first panels that fail and `p` the share of licences that release escrow:

  ```
  claims per month = 2 × DAA_month / [ (1 − f)·(26 + 121·(1 − p)) + f · H_f ]
  ```

  `H_f` = 753 DAA without SR-9 (600 of receipt window, then the held second panel), about 173 DAA with SR-9
  **(model: 26 DAA to the third Unavailable, then 147 held)**. Option A is the same formula at `p = 0`. With t11's
  measured 30% per-remote-seat Withheld (the (dos) STATE:12847 comment):

  | case | f | p | v3 without SR-9 | option A | **v3 with SR-9 (model)** | option A with SR-9 (model) |
  |---|---|---|---|---|---|---|
  | one co-located seat, 120 s / 200 s | 0.0837 | 0.262 | 256 / 154 | 218 / 131 | **360 / 216** | 290 / 174 |
  | five remote seats, 120 s / 200 s | 0.1631 | 0.201 | 192 / 115 | 176 / 105 | **330 / 198** | 286 / 171 |
  | no failures (f = 0), p = 1 / 0.5 / 0.24 / 0, 120 s | 0 | | 1,662 / 499 / 366 / 294 | 294 | | |

  v2's "+24% over today" reading is dropped. Without SR-9 the gain over option A is 9–17%; SR-9 is what makes
  principle 6 ("throughput caps, not bond caps") real for newcomers below p = 1. p stays griefable: a silent seat
  holds the escrow for free (SR-1b). The p model counts a seat as serving when it signs `Valid`; since `Sampled` no
  longer serves (C1), a partial seat that only sampled by its receipt deadline also lowers p. O-2 measures f and p
  live.
* **(e) Network licence capacity** is Σ over seats `⌊(0.5·C − committed)/max(duty, lock)⌋ / (signers × H)`. For the
  eight genesis seats on the floor: the V1 door (3 signers × `lock_3`) gives 3.76 claims/DAA at H = 3,121 and 1.29 at
  H = 9,121, as in v2. **The coverage door (5 signers × `lock_2`) gives 1.50 and 0.51.** On a quiet chain (H = 9,121)
  genesis-seat capacity for coverage licences is below the floor lane's ~1 claim/DAA. Named, not changed; O-2 watches it.

**T-3 (unminted-reward ceiling).** Owner A (S). Tests T03, T16, T55, T56.
* (a) Per bond: unlicensed escrow ≤ 500‰ (2 claims on 13,000 MSK, 20 on 130,000). K = 64 for the genesis seats is
  nominal under the one ledger (X28).
* (b) Network: `Σ vesting ≤ 3,200.85 × Finals in the last 121 + 3,000 (+6,000) DAA` = 9.99M (29.19M) MSK, plus the rows
  a DA session has re-keyed (each at most to its claim's `trace_retention_daa + window_challenge_at`, DA-5); frozen
  during a halt. Checked by the V-3 counters. **A DA session can no longer freeze the other rows (V3S-02):** the
  session's row is re-keyed out of the head of the order when the session opens, so the ceiling is not defeated by
  a session held open on the head row (T16, T82).
* (c) Rate: at most 8 new queue keys a block (V-7).

**T-4 (2M weight).** Owner A (text). Test T20. Unchanged: 59,742.94 MSK of weight is priced, not recovered; one 2M Final carries 555,748× a
floor Final's weight; the reorg value of weight has no consensus bound. **The 2M row does not satisfy the invariant**
(§4). ADR-0153 comes before any `c_2M ≥ 2`.

### 3.8 W — withholding (audit requirement 7)

Owner A (fold, M3), B (filer, P2-6). Tests T34, T57, T77.

* Withholding is charged when **DA-confirmed** (S1), and, until X10, when it sinks both panels (S0′).
* **DA on every class (C-1).** The fold accepts `DefaultAccused` on every class, floor included (DA-1). T34 pins it.
  Phase 2 files automatically (P2-6): a seat whose fetch has failed files `Unavailable` and a `DefaultAccused`
  naming the missing unit **before its receipt window closes** (at `bound + window_receipt − 60`), and at once when a
  licence lands on a claim that did not serve it. A seat session pauses the claim (DA-5), so a withholding producer
  cannot take its free redraw without first answering (DA-4) or defaulting (S1).
* **Amplification, DA-confirmed** (5 × duty pinned ÷ forfeit):

  | class | today | v2 | **v3** |
  |---|---|---|---|
  | floor | 0.3993 | 0.3993 | **0.4000** |
  | 8k | 2.01 | 0.414 | **0.6212** (duty `lock_2` = 459.12) |
  | 2M | 14.24 | 0.020 | **1.0000** (duty capped at `(w+E)/5`) |

  The bound that matters is ≤ 1 (F14): a withholder never pins more than it forfeits. v2's floor figure 0.3993 used the
  one-attempt fixture's `w`; 0.4000 is the genesis row's.
* **Unaccused withholding.** At launch a claim that fails two panels forfeits (S0′); one failed panel redraws early
  under SR-9. After X10 unaccused withholding is free and rests on filing. `dos_repro_4` is re-run with automatic
  accusation (T34).

### 3.9 P0-10 on t12, per attacker strategy (audit requirement 1; F5)

Owner B (text; filers in P2-6/P2-8/P2-8b), A (fold paths, M2/M3). Tests T18, T18b, T18c, T34; O-3.

**A second open item, which gates launch (post-edit 8).** An arithmetic lie in an `AttnFused` step leaf on a
held-context class (8k, 2M) has **no conviction route** today: the leaf checker returns `NeedsDissection`, a named
fused leaf is refused as a DA unit (DA-3) and routed to a held dissection, and no close of that dissection convicts the
lie (J-8's court door also refuses a decode close narrowed at an `AttnFused` leaf). The garbage strategy below is therefore unattributable on 8k and 2M when the lie sits
in a fused-attention leaf. The audit session is doing the design pass on (A) a chunked attention proof, (B) K/V rows as
committed outputs plus held-DA, and (C) an economic bound; if (B) is chosen it touches producer/engine commitments
and P2-7's retention duty. Launch waits on it (§8.3 item 7).

**Decided (2026-09-24, the operator on the audit's analysis; the full specification is the audit's Section 4-ter,
`attention_attribution_verdict.md` §2).**
* **8k: A-held, in the launch gate** (the 8k half of F1-M (a)). The existing k-ary dissection is completed for held
  classes, all under `palw_offence_attribution`: **C1** no mercy for a held class with `n_ctx ≤ 8,192`; **C2** a new
  object `CourtAttnRootClaimedHeld` (**tag 57**, after S's 53–56), whose root claim carries every anchor-checkpoint
  slice sub-root (56 × 64 B on A16) — C1 ships only with C2, because the per-position anchor sits at p+1 and a
  consistent forger otherwise leaves the challenger no bottom; **C3** a one-move accusation and `CheckpointAccused` are
  accepted during a dissection, and a void closes the session neutrally (one court per claim must not become a shield
  for a Sybil decoy dissection); **C4** an accuser who loses a held dissection is charged `max(reserved, G)`, capped at
  13,000 MSK; **C5** a held class with `n_ctx > 8,192` is refused at registration. No commitment-format change and no
  v22 field (partial-mask signers are not bound at launch, so there is no `site_leaf`).
* **The producer's new duty (8k).** A producer facing a held dispute must answer with `CourtAttnRootClaimedHeld` within
  the response deadline (42 DAA as specified; see the deadline classes below) or is convicted. The engine needs a
  windowed builder (one forward to p+1 and openings built from the fold-retained digests), and `supports_dissection()`
  reads the held size: a build without it does not produce 8k. The windowed re-execution of 8,192 positions is
  estimated at 15–35 min and unmeasured, so a real-weight 8k timing drill is part of the launch drill (§8.3).
  Owners: A for the consensus half (C1–C5, object 57, the deadline and the silent-conviction rule) and the base0
  builder; B for kaspad's automatic answer (Phase 2's node automation).
* **2M: not attributable at launch, accepted as (C).** Uncovered: 63,599 MSK per claim, `c_2M = 1`, at most 9.35M MSK a
  month. The escrow hold to Final and RT#2's forfeiture stay. An attention lie found only after Final moves to Phase 2.
  **Superseded for the launch by U-D1 (below; IA-12): 2M is closed at launch.**
* **Deadlines by kind (operator direction; values after measurement).** Rather than one 42-DAA extension everywhere,
  deadlines split into (1) routine responses (signing, filing, disclosing an existing checkpoint), (2) compute-bearing
  verification (S1 segment replay, court re-execution, building evidence), derived from each class's measured cost,
  and (3) economic liability (vesting, the slashable period, escrow), never released before verification can finish.
  2M's economic Final stays disabled until an honest verifier can prove a lie inside the deadline. The audit is
  inventorying every timer; the drill host measures the 2M full replay, the slowest S1 segment, checkpoint build and
  load, and detection-to-evidence time. Whether 8k's 42-DAA duty is class (1) or (2) is fixed by the 8k timing drill.

**Decided later on 2026-09-24 (the operator, on the audit's deadline design, `deadline_2m_design.md` Section 4-quater;
all recommended options; IA-12).** 4-quater makes the deadline direction above concrete:
* **Kinds.** R (response: sign, submit, open retained state), C (compute-bearing verification: seat replay, S1
  segment, building evidence), E (economic liability: escrow, reward, lock, retention duty, rights maturity) and K
  (capacity, nobody's deadline).
* **Only C is derived per class,** as `D(c)` (on the FP lane from the job's length too). Every economic release waits for
  `H(c) = B*(c) + D(c) + 1`, where `B*` is the bind plus the seat-DA pause V6 credits back. The one exception is
  SR-1's escrow release when every seat carried a `Valid`, because then no seat is left verifying.
* **What stays global.** The 42-DAA turn, the 3,000-DAA court window and `W_disclose` = 1,200 stay global: each is a
  response once the retained-state duties hold and the panel loop never waits on compute.
* **The consensus half** goes under a new fence, `palw_class_verify_deadline` (t12 at genesis; t11, devnet and mainnet
  `None`), with no v22 field:
  * V1 the function `D(c)`, which also fixes the span unit: the registry prices a span at 600 s while t12 counts it as
    1 DAA of 120 s, so 2M's receipt window is 2,799 DAA against the 13,995 derived;
  * V2 a class needing a measured row and without one is refused (`ClassDeadlineUnmeasured`), which closes 2M;
  * V3 the per-claim receipt window;
  * V4 the Final floor at all five sites through one helper;
  * V5 the licence-time lock;
  * V6 the DA pause moves H;
  * K-1 a long-D class owes its panel room until Final.

| # | decision |
|---|---|
| **U-D1** | **2M is closed at launch**, attempt and FP. It opens only at a flag day that installs measured params rows, and only if it is attributable (F1–F4 GREEN, with 4-ter's C1 extended to 2M) **and** an honest seat can prove a lie within D (M2, M6 measured); that flag day needs every one of 4-quater's E-1…E-11. Until then 2M's economic Final stays disabled. This supersedes (C) above |
| U-D2 | The "needs a measured row" set is D > 600, or a held class with `n_ctx` > 8,192, or a class whose working set pages |
| **U-D3** | **D_cap 16,000 and the pruning depth fixed at the regenesis at ≈ 74,920 DAA (P-1;** ESTIMATED in the design, its `numbers_final.py`: the lattice `2(600 + W) + 1,200 + 3,000 + P` with `P` the `R_eff` span**).** A pruning depth cannot be raised later, since a pruning point cannot move back. At today's 12,002 the largest window that fits is 601. The design asked for M12 to check the storage and IBD cost first; **the operator decided not to wait for M12** (drills and measurements after launch) |
| U-D4 | Margin m = 2 and τ = 120 s (the target) for the conversion; no MTP floor unless M7 fails |
| U-D5 | 2M is FP-only (no 2M attempt can become a block: the job is bound to its template and a canonical run takes days); the bond ceiling limits N |
| U-D6 | Design A: on a class that needs a measured row, all five seats run the full replay (node policy) |
| U-D7 | K-2 and K-3 (capacity) go in when 2M opens |
| U-D8 | Never open unattributable: E-7 is mandatory, and 2M stays closed if M6 fails |
| U-D9 | RT#2 is kept for 2M after it opens, and griefing is priced by E-10 |
| **U-D10** | **Compute-bearing replays leave the kaspad panel loop before launch** (N-4, node only) |

* **Measured after the design (M13; B, `measure/t12-m13-2m-fp-price` `3e6a2220`, as the audit's design doc §6
  records it):** rr / w = 505.759 for every class and N. A 938,888 MSK bond covers an FP claim up to N = 22,598.
* **Status on the integration line.** The consensus half (the fence, V1–V6, K-1, P-1) is **not** there; it is a ship
  condition (§8.3). The node half is partly there: SEAT-R (`b589622f5`) ships N-1 (the duty's deadline is the chain's
  per-claim receipt window) and N-4 (U-D10: replays run as detached tasks off the loop). The seat work (`1e20edd50`)
  ships N-5 (the material wait capped at `min(W_r/2, 60)`), N-6's R2 retention to `R_eff`, and N-9's per-claim
  deadline display.
* **The 8k timing drill runs after launch (IA-12).** Per the operator's decision, drills and measurements run on
  public t12 after launch (§8.4). That includes the 8k real-weight timing drill, so "fixed by the 8k timing drill"
  above is answered after launch.

**Status: OPEN. Priced, not closed.** `class_ticket_v3` (`palw_attempt_v2.rs:581`) is a function of
`execution_commitment_v3` (`:562`), which covers declared roots. The free tag is safe only with the exposure cap and a
charge on failure. Closing P0-10 needs a ticket that is a function of a certified execution (ADR-0141 §2), not
feasible in this regenesis. The reservation MUST NOT go below the gain until that exists.

| strategy | what the attacker commits | attribution path | charge at launch (X10 dormant) | charge after X10 |
|---|---|---|---|---|
| **naive** | roots with no material behind them; never answers | RT#2; DA default if anyone accuses | `w + E` at RT#2 (S0′), or S1 + strike if a DA session defaults first | S1 if filed (Phase 2 auto-DA), else 0. Filer's net: +R (320.10 on the floor), exposure returned |
| **garbage** | the correct job's context, a well-formed garbage trace; answers DA | DA disclosure passes the identity checks J1–J5 → StepLeaf demand at a divergent leaf → StepArithmetic / held dissection → `ExecutorRefuted = 4` (J-6); garbage logits on an honest step tree → `LogitsNotStepOutput = 12` (F1c, in the gate) | `w + E` at RT#2 (honest seats refuse Valid), or S2 if refuted first | S2 if a filer builds the contradiction (the capture arm, P2-8; the replay-mismatch builder, P2-8b, for every seat), else 0. **Filer's net (V3S-06):** the ≥ 2 refuted sessions of the path (2 × 320.10 on the floor, 2 × 369.52 on 8k, 2 × 6,294.38 on 2M) are held and refunded when the claim is convicted (DA-6), so a correct filer nets ≥ 0, plus R unless the offender pre-committed (R-3). The v3 draft left it at −190.10 / −239.52 / −17,472.80 MSK |
| **borrowed** | another job's genuine roots (or a relabelled honest run, on the floor and, with F1-M, on 8k and 2M) | every DA answer fails the identity checks (J1 `job_id ≠ job_identity`, or J5's full context) → S1; or `ExecutorRefuted{IdentityMismatch}` from anyone holding the binding (root 0, forfeit by claim, so the lender keeps its rights) | `w + E` at RT#2, or S1 / S2 first | S1 / S2 if filed; filer's net ≥ 0 as above |

* **Lane capture per day** (floor, 1 claim/DAA): at launch as today, 2,304,684 MSK/day at 120 s/DAA and 1,382,810 at
  200 s/DAA burned under RT#2 (review re-run of `dos_repro_4`). After X10 the same only if every junk claim is
  attributed; unattributed it is 0 with 7,707,635 MSK of refundable collateral (review).
* **2M lane:** one junk 2M claim holds the `c = 1` lane for 5,602 DAA (`dos_repro_3`). Under D7 it forfeits
  62,943.79 MSK (S0′), about 8,090 MSK/day at 120 s, before and after X10.
* **8k lane (post-edit 5, correcting V3S-09):** the 8k row is not in C7, so it is room-governed like any class outside
  C7: a junk 8k claim owes its replay against the 8k rate room until its second panel fails (a licence would free it,
  but a junk claim never licenses), and forfeits 3,695.17 MSK (S0′) at launch. No static cap binds it past the grace.
  After X10 it is charged only if attributed.
* **Pause credit is not a capture tool (V3S-08).** Only a session opened by a seat of the claim's panel pauses a
  pre-Final claim (DA-5). In the v3 draft a producer's own non-seat Sybil could open sessions on its junk claim,
  answer each at `deadline − 1`, and hold the room for up to the retention bound: on 8k about 0.84 MSK per slot-DAA
  against 3.03 unpaused, 3.6× cheaper (verifier model, reproduced by `v3review/extra.py`). A producer's Sybil that is
  a seat of the panel can still pause, but the honest seats of the same panel were withheld from and file their own
  sessions (P2-6), which lead to J-6's conviction.
* **X10's arming criterion** is exactly this table's right column working live: O-3 runs an adversarial producer with
  the garbage and borrowed strategies on public t12 and reports `q_f` (§8.4).
