# v2 ソース確認記録

対象: 添付 `misakas-pre(2).zip` の展開 snapshot。live branch、出荷 binary、現在の t12 は確認していない。

確認日: 2026-10-19。下記は文書・コード読解の証跡であり、コード中の試験を今回実行した結果ではない。

前版の認証・算術の監査は [reference-v1/source-audit.md](../0024-assets/source-audit.md) を参照。

**17,408 B は依頼中の提案値として扱う。** `17_408 / 17408 / 17,408` のソース・文書検索ではモデル FFN 幅等は見つかったが、これを現行 court step サイズの定数と裏付ける一致は確認できなかった。したがって既存値からの無変更とは扱わない。

**既存値との不一致:** snapshot の `DEFAULT_MAX_CLOSE_BYTES` に対する試験は 2,250,000 B。新しい 64 KiB の court 総量目標を既存実装が満たしているとは解釈しない。

<a id="v2-s01"></a>

## V2-S01 — 既存 frame は collect / material / bond exit を一緒に導出する

`consensus/core/src/palw_tir_public_allocation_v1.rs` L733–L758  
SHA-256: `f98103deec92feaa6c40dd2bb44b43d76d1fa63604845eaa40689027ea6ba5ff`

```text
   733 |     let generic_end = add(open, post_inclusion.max(1))?;
   734 |     let stream = if let Some(quote) = &mut semantic {
   735 |         let shift = generic_end.checked_sub(geometry.accepted_daa).ok_or("native semantic offset underflow")?;
   736 |         rebase_new_semantic_plan(&mut quote.plan, shift)?;
   737 |         sum = sum.checked_sum_v1(quote.plan.terms.plan)?;
   738 |         quote.plan.terms.stream_until_daa
   739 |     } else {
   740 |         generic_end
   741 |     };
   742 |     let collect = add(stream, rules.config.collection_daa)?;
   743 |     let material = add(collect, rules.config.reorg_retention_daa)?;
   744 |     let terms = PalwRenderingClaimTermsV1 {
   745 |         accepted_daa: geometry.accepted_daa,
   746 |         open_until_daa: open,
   747 |         stream_until_daa: stream,
   748 |         collect_until_daa: collect,
   749 |         material_until_daa: material,
   750 |         bond_exit_after_daa: material,
   751 |         plan: sum,
   752 |     };
   753 |     terms.plan.rent_sompi.checked_add(terms.plan.deposit_sompi).ok_or("native full allocation capital overflow")?;
   754 |     let mut plan = PalwTirPublicAllocationPlanV1 {
   755 |         version: VERSION,
   756 |         source_id: source.source_id_v1(),
   757 |         original_claim: crate::palw_freeprompt_v3::fp_claim_id_v3(original),
   758 |         rules_id: rules.commitment_v1()?,
```

<a id="v2-s02"></a>

## V2-S02 — 現行期限は受付時に固定され、collect_until は排他的境界

`consensus/core/src/palw_tir_public_fold_v1.rs` L889–L934  
SHA-256: `9092c26113c4636ee8ea7bbcc3550f1648c0134e7cbeccd59b723d42c20531ef`

```text
   889 |     }
   890 |     Err(refused("TIR public output conviction never reached the original authenticated EOF"))
   891 | }
   892 | 
   893 | /// **G14 R4/R6: one public bonded verifier convicts an ORIGINAL frame from authenticated public material alone.**
   894 | ///
   895 | /// 1. The filer is any non-liable Active above-floor bond ([`conviction_filer_v1`]); the window is the frame's own original
   896 | ///    collection door `[accepted, collect_until)` — fixed at d0, never extended — inside which the claim cannot reach `Final`
   897 | ///    (`palw_claim_final_floor_v1` floors it at `collect_until`), so no Final weight, receipt spend, escrow, Gate C credit or
   898 | ///    coinbase leg can escape before this door closes.
   899 | /// 2. The evidence is adjudicated natively ([`crate::palw_tir_objective_v1`]): the registered program filled from state, the
   900 | ///    original class/artifact/execution/trace/job pinned from the claim, the court rules and the step ladder the native ones,
   901 | ///    the proof body bounded by the producer-prepaid terminal witness ceiling. `PredicateHolds` / `OutputHolds` refuse the object
   902 | ///    (nothing changes; it is neither guilt nor a certificate of honesty). Errors (`Need`, missing material, oversize, EOF short)
   903 | ///    refuse it too — none of them is read as a finding.
   904 | /// 3. The sanction reuses the established economic funnels, never a new tariff: every credited Valid custodian's S4 (its native
   905 | ///    lock plus the S3/S4 action tier, keyed `palw_false_valid_offence_id_v2(seat, claim)`, so once only across this route, the
   906 | ///    kind-3 route and the DA route), then the producer through `convict_by_court_verdict_v1` (void a live claim `CourtFraud`,
   907 | ///    or reverse a `Final` one — which revokes its RFC-0018 Gate C weight and frontier/settlement credit by the one predicate —
   908 | ///    and record `CourtConviction` once, the filer's reporter reward, by-claim round-right forfeiture and ADR-0160's intent-class
   909 | ///    funnel). An execution-proving fault also forfeits every minted right by ROOT, so a sibling claim sharing the convicted
   910 | ///    execution (and an EXEC root's dependent slices, through `void_claim`) loses its standing too.
   911 | /// 4. Every liable bond must still hold its original exit hold (the frame's `staged_holds`), so its collateral is locked when the
   912 | ///    conviction opens and the debit is actually collectible (`conviction_collected_v1`); a liable bond without it refuses the
   913 | ///    filing instead of recording a fictitious collection.
   914 | fn apply_conviction_v1(
   915 |     b: &mut TransitionBuilder<'_>,
   916 |     ctx: &PalwBlockContextV2,
   917 |     frame: &Frame,
   918 |     filer: PalwBondKeyV2,
   919 |     evidence: &crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
   920 | ) -> Result<(), PalwStateV2Error> {
   921 |     use crate::palw_tir_objective_v1::{
   922 |         PalwTirObjectiveOutcomeV1 as Arith, PalwTirOutputFaultV1 as OutFault, PalwTirOutputOutcomeV1 as Out,
   923 |     };
   924 |     use crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1 as E;
   925 |     let claim_id = frame.plan_v1().original_claim;
   926 |     let terms = frame.plan_v1().terms;
   927 |     if ctx.daa_score < terms.accepted_daa || ctx.daa_score >= terms.collect_until_daa {
   928 |         return Err(refused("TIR public conviction is outside the frame's original collection door"));
   929 |     }
   930 |     conviction_filer_v1(b, frame, filer)?;
   931 |     let claim = b.state.claim(&claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
   932 |     let producer = frame.custody_v1().pledge_v1().producer;
   933 |     if claim.bond != producer || claim.execution_root != frame.original_v1().execution_root {
   934 |         return Err(refused("TIR public conviction target changes the original claim's producer or execution"));
```

<a id="v2-s03"></a>

## V2-S03 — 保持・bond・署名済み pledge の期限整合

`consensus/core/src/palw_tir_public_custody_v1.rs` L344–L359  
SHA-256: `ac1a78e1474bf3dbe5a128574456ee7b49c12c7edb1f1d7c6658d72390055865`

```text
   344 |     }
   345 |     pub fn check_v1(&self) -> Result<(), String> {
   346 |         self.pledge.check_v1()?;
   347 |         let t = self.terms;
   348 |         if self.drawn_panel.contains(&self.pledge.producer)
   349 |             || (self.drawn_panel.len() as u64).checked_add(1).is_none_or(|n| n > self.pledge.maximum_custodians)
   350 |             || t.accepted_daa >= t.open_until_daa
   351 |             || t.open_until_daa >= t.stream_until_daa
   352 |             || t.stream_until_daa >= t.collect_until_daa
   353 |             || t.collect_until_daa > t.material_until_daa
   354 |             || t.material_until_daa > t.bond_exit_after_daa
   355 |             || t.accepted_daa >= self.pledge.accept_until_daa
   356 |             || t.material_until_daa > self.pledge.material_until_daa
   357 |             || self.grants.iter().any(|(bond, (credited_daa, grant))| {
   358 |                 !self.drawn_panel.contains(bond)
   359 |                     || *bond != grant.seat
```

<a id="v2-s04"></a>

## V2-S04 — owner 枠上限と idle 回収。全枠 active の場合は拒否

`consensus/core/src/palw_tir_public_fold_v1.rs` L617–L643  
SHA-256: `9092c26113c4636ee8ea7bbcc3550f1648c0134e7cbeccd59b723d42c20531ef`

```text
   617 | 
   618 | fn open_v1(
   619 |     b: &mut TransitionBuilder<'_>,
   620 |     ctx: &PalwBlockContextV2,
   621 |     frame: &Frame,
   622 |     owner: PalwBondKeyV2,
   623 | ) -> Result<(), PalwStateV2Error> {
   624 |     independent(b, frame, owner)?;
   625 |     // **K_owner (Lead default, pending user review): refuse an excess owner before ANY custodian duty is created.** The frame's
   626 |     // liable custodians answer at most `custodian_load_bound_v1` across the owners holding slots at once; each owner below pays
   627 |     // only its own rent and deposit (`owner_terms_v1`), never the producer's prepaid preparation pool.
   628 |     //
   629 |     // **G-T1 (Lead default R1, idle-slot reclamation; the cap stays K_owner = 8): an idle owner never locks an honest verifier
   630 |     // out.** At the cap, this Open reclaims the slot of the holder idle longest (no move for one native acquisition span since its
   631 |     // last progress, `owner_idle_at_v1`): that row is retired (its open demand closed, its turns spent — it can never demand again),
   632 |     // and its deposit is forfeited (burned through `slash_bond`, credited to nobody, so reclaiming pays no one). Only when every
   633 |     // holder is still making progress is the Open refused. Atomic with the Open (the caller's checkpoint).
   634 |     let claim = frame.plan_v1().original_claim;
   635 |     if b.state.staged_tir_owner_count_v1(&claim) >= Frame::MAXIMUM_OWNERS_V1 {
   636 |         let Some((idle_key, idle_owner)) = b.state.staged_tir_idle_owner_v1(&claim, ctx.daa_score) else {
   637 |             return Err(refused("TIR original frame already carries its K_owner outside owners, all still active; custodian load is bounded"));
   638 |         };
   639 |         reclaim_owner_slot_v1(b, ctx, idle_key, idle_owner)?;
   640 |     }
   641 |     let c = owner_contract(frame, owner, ctx.daa_score)?;
   642 |     let key = owner_key(frame, owner, ctx.daa_score)?;
   643 |     let allowance = rules(b)?.config.maximum_owned_rows_per_reserver;
```

<a id="v2-s05"></a>

## V2-S05 — owner 上限の既存定数

`consensus/core/src/palw_tir_public_claim_v1.rs` L265–L283  
SHA-256: `4d53e8e159484c8b50c48434e4b07c7162bc27b3db91ed761e47f3c35dcd8266`

```text
   265 |         now: u64,
   266 |         native_network_domain: Hash64,
   267 |     ) -> Result<(), String> {
   268 |         self.check_structure_v1(&self.key_v1())?;
   269 |         self.custody.qualify_original_native_v1(state, source, preparation, receipt, grant, now, native_network_domain)
   270 |     }
   271 | }
   272 | 
   273 | /// **K_owner — outside owners per ORIGINAL TIR frame** (Lead default 2026-10-15, pending user review). Each owner pays its own
   274 | /// rent and deposit (never the producer's prepaid pool); the cap bounds what the frame's liable custodians must answer. Known
   275 | /// residual: a colluder can pre-empt the slots by paying K owners' rent and deposit; slot recycling for idle owners is a
   276 | /// follow-up decision (recorded in the remaining-work document).
   277 | pub const PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1: u64 = 8;
   278 | 
   279 | /// The bound on what one liable custodian of a frame can be asked to answer (see [`PalwTirPublicClaimV1::custodian_load_bound_v1`]).
   280 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   281 | pub struct PalwTirPublicCustodianLoadV1 {
   282 |     pub owners: u64,
   283 |     pub demands_per_custodian: u64,
```

<a id="v2-s06"></a>

## V2-S06 — pruning snapshot と未完 close group の bytes

`consensus/core/src/palw_state_v2.rs` L22678–L22715  
SHA-256: `ce6a025a3457b8505b0e328ebd75d447b2397256831916d6d971647b0ffd4fa7`

```text
 22678 | /// The serializable snapshot of a state's PRIMARY data — what the pruning proof carries
 22679 | /// (Decision 5) so a pruned node can continue exactly where an archival node stands. Indices are
 22680 | /// never serialized: [`PalwStateCarriageV2::into_state`] rebuilds them and refuses a snapshot
 22681 | /// whose derivable facts disagree with its claims.
 22682 | #[derive(Clone, Debug, PartialEq, Eq)]
 22683 | pub struct PalwStateCarriageV2 {
 22684 |     pub version: u16,
 22685 |     pub bonds: BTreeMap<PalwBondKeyV2, PalwBondStateV2>,
 22686 |     pub reserved_exposure: BTreeMap<PalwBondKeyV2, u128>,
 22687 |     pub classes: BTreeMap<Hash64, PalwClassStateV2>,
 22688 |     pub class_targets: BTreeMap<Hash64, PalwClassTargetV2>,
 22689 |     pub class_shares: BTreeMap<Hash64, u16>,
 22690 |     /// ADR-0056 Decisions 3 and 5. Carried like every other accumulator so a syncing node rebuilds
 22691 |     /// the same root; both are re-derived and compared by `assert_internal_consistency`.
 22692 |     pub registration_exposure: BTreeMap<PalwBondKeyV2, u128>,
 22693 |     pub class_walks: BTreeMap<Hash64, PalwClassWalkV2>,
 22694 |     pub epoch_budgets: Option<PalwEpochBudgetsV2>,
 22695 |     pub receipt_targets: BTreeMap<Hash64, PalwClassTargetV2>,
 22696 |     pub capabilities: BTreeMap<Hash64, PalwCapabilityStateV2>,
 22697 |     pub claims: BTreeMap<Hash64, PalwClaimStateV2>,
 22698 |     /// Carried because `state_root` hashes it: a snapshot that dropped the queue would restore a
 22699 |     /// state whose root does not match the chain's, and a node loading it would find every
 22700 |     /// subsequent block's coinbase wrong.
 22701 |     pub pending_payouts: BTreeMap<Hash64, PalwPayoutV2>,
 22702 |     pub certified_families: BTreeMap<Hash64, PalwCertifiedFamilyStateV2>,
 22703 |     pub fp_certified_families: BTreeMap<Hash64, PalwCertifiedFamilyStateV2>,
 22704 |     pub fp_certified_classes: BTreeMap<Hash64, PalwClassLaneCertificationV2>,
 22705 |     pub pending_chunks: BTreeMap<Hash64, PalwPendingChunksV2>,
 22706 |     /// **ADR-0080 design A: the declared closes, carried with their BYTES.**
 22707 |     ///
 22708 |     /// A pruned node that could not complete a group a declaration already pinned would compute
 22709 |     /// a different state root from an archival one at the completing block, which is the split
 22710 |     /// this snapshot exists to prevent — the same argument `pending_chunks` is carried under.
 22711 |     pub court_close_groups: BTreeMap<(Hash64, PalwCourtSideV1), PalwCourtCloseGroupV2>,
 22712 |     /// ADR-0078 Decision 4: the derivation table, carried because the root hashes it.
 22713 |     pub derived_artifacts: BTreeMap<crate::palw_derived_v1::PalwDerivedKeyV1, crate::palw_derived_v1::PalwDerivedRowV1>,
 22714 |     pub panels: BTreeMap<Hash64, PalwPanelStateV2>,
 22715 |     pub court_sessions: BTreeMap<Hash64, PalwCourtSessionStateV2>,
```

<a id="v2-s07"></a>

## V2-S07 — 既存 close 上限は単一 step fragment 上限とは異なる

`consensus/core/src/palw_mode_v2.rs` L534–L566  
SHA-256: `8959aeafe2396b03666f9bd5e62d691e78f258ae176b6dc9c4d0383a40eaac3a`

```text
   534 | /// `fix/audit-batch-1` lowered this same field from 1 MiB for the right reason — a close no node
   535 | /// would relay makes a court that "looks configured and is unreachable" — but sized it against
   536 | /// `compute_mass` alone (`mass_per_tx_byte` is 1, so 480,000 mass reads as 480,000 bytes). The
   537 | /// mempool checks BOTH arms: `check_transaction_standard.rs` rejects on `transient_mass` too, and
   538 | /// that arm is `size x 4`. So a TRANSACTION holds 120,000 bytes, not 480,000. That is why the
   539 | /// per-chunk number is 100,000 and not 400,000; the count above is what makes the ceiling large,
   540 | /// and it is large by carrying more transactions rather than by mis-reading one.
   541 | pub const DEFAULT_MAX_CLOSE_BYTES: u64 = palw_close_bytes_for_chunks_v1(DEFAULT_MAX_CLOSE_CHUNKS);
   542 | 
   543 | /// **How many chunk-group parts one court close may span** (ADR-0080 design A).
   544 | ///
   545 | /// The one number a network chooses in this derivation; [`DEFAULT_MAX_CLOSE_BYTES`] is its
   546 | /// consequence. 27 is what admits the dense A16 row at a context wide enough for ADR-0078's
   547 | /// artifact demonstration to come out of a real inference: that row's binding node closes at
   548 | /// 1,154,673 bytes at `n_ctx` 512, which is 14 chunks framed, and the hybrid tier's is 2,240,241,
   549 | /// which is 27. Rounding the count DOWN to the dense row alone would have priced a court that
   550 | /// cannot prosecute the class the same ladder admits.
   551 | pub const DEFAULT_MAX_CLOSE_CHUNKS: u64 = 27;
   552 | 
   553 | /// **The framing factor, as a fraction.** The encoded object runs about 1.20x the bytes the
   554 | /// ceiling counts (measured 90,888 borsh against 77,568 counted), so a close of `n` counted bytes
   555 | /// occupies `n * 12 / 10` carried bytes. Spelled as a numerator and denominator rather than as a
   556 | /// float because this is consensus arithmetic and a float would make the ceiling platform-shaped.
   557 | pub const PALW_CLOSE_FRAMING_NUMERATOR: u64 = 12;
   558 | /// See [`PALW_CLOSE_FRAMING_NUMERATOR`].
   559 | pub const PALW_CLOSE_FRAMING_DENOMINATOR: u64 = 10;
   560 | 
   561 | /// The counted bytes `chunks` chunk-group parts carry, after framing — the derivation
   562 | /// [`DEFAULT_MAX_CLOSE_BYTES`] is.
   563 | pub const fn palw_close_bytes_for_chunks_v1(chunks: u64) -> u64 {
   564 |     let carried = chunks.saturating_mul(crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64);
   565 |     carried.saturating_mul(PALW_CLOSE_FRAMING_DENOMINATOR) / PALW_CLOSE_FRAMING_NUMERATOR
   566 | }
```

<a id="v2-s08"></a>

## V2-S08 — 既存 DEFAULT_MAX_CLOSE_BYTES のテストは 2,250,000 B

`consensus/core/src/palw_mode_v2.rs` L1743–L1757  
SHA-256: `8959aeafe2396b03666f9bd5e62d691e78f258ae176b6dc9c4d0383a40eaac3a`

```text
  1743 |         let chunk = crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64;
  1744 |         assert!(chunk < payload_budget, "one chunk ({chunk} bytes) must fit a standard transaction ({payload_budget})");
  1745 |         let carrier_overhead = payload_budget - chunk;
  1746 |         assert!(carrier_overhead >= 18_000, "the carrier needs room of its own beside the chunk, got {carrier_overhead}");
  1747 | 
  1748 |         // The group half: the ceiling is the count and nothing else.
  1749 |         assert_eq!(DEFAULT_MAX_CLOSE_CHUNKS, 27);
  1750 |         assert_eq!(DEFAULT_MAX_CLOSE_BYTES, 2_250_000, "floor(27 x 100,000 / 1.20)");
  1751 |         assert_eq!(DEFAULT_MAX_CLOSE_BYTES, palw_close_bytes_for_chunks_v1(DEFAULT_MAX_CLOSE_CHUNKS));
  1752 | 
  1753 |         // And the two directions are one arithmetic: a close at the ceiling needs exactly the
  1754 |         // chunks the ceiling buys, and one byte more needs one more.
  1755 |         assert_eq!(palw_close_chunks_for_bytes_v1(DEFAULT_MAX_CLOSE_BYTES), DEFAULT_MAX_CLOSE_CHUNKS);
  1756 |         assert_eq!(palw_close_chunks_for_bytes_v1(DEFAULT_MAX_CLOSE_BYTES + 1), DEFAULT_MAX_CLOSE_CHUNKS + 1);
  1757 |         assert_eq!(palw_close_chunks_for_bytes_v1(1), 1, "a close is never free of its first chunk");
```

<a id="v2-s09"></a>

## V2-S09 — leaf 数・close bytes・work・operand 上限は別々

`consensus/core/src/palw_class_admission_v2.rs` L116–L146  
SHA-256: `beba8e69955c352eb1834ae201a0952801a18c9aa2b8107a39461817d35b489f`

```text
   116 | /// job, chosen because the cost of the ladder is paid by every honest prosecution.
   117 | ///
   118 | /// This value is inside `palw_ruleset_id_v2` and therefore inside the fingerprint. It is free to
   119 | /// move on a wipe relaunch and impossible to move afterwards, which is why it is being decided
   120 | /// before the cut rather than discovered from a user who cannot get a MIDI file out.
   121 | pub const PALW_RC_COURT_MAX_STEP_LEAF_COUNT: u64 = 1 << 26;
   122 | 
   123 | /// **The three cost ceilings an RC identity must freeze, and why they are constants rather than
   124 | /// an operator's choice** (ADR-0049 Decision C; the second of the road map's two decisions that
   125 | /// expire).
   126 | ///
   127 | /// `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` above bounds how many rounds a dispute takes. These bound
   128 | /// what a round COSTS, and they sit in the same place for the same reason: they are
   129 | /// `PalwConsensusParamsV2` fields, so they are inside `palw_ruleset_id_v2`, so a class that exceeds
   130 | /// them cannot join a running chain — it needs a new ruleset, which is a flag day. The ladder gate
   131 | /// was already a refusal in `assemble_palw_rc_identity_v2`; these were not, and an RC genesis could
   132 | /// be minted with any ceiling at all, including the generous default nobody had checked against a
   133 | /// transaction.
   134 | ///
   135 | /// They are the shipped defaults, restated here as an RC commitment. Keeping two names is
   136 | /// deliberate: [`crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES`] is what a bundle gets when a caller
   137 | /// does not say, and this is what the RC network's identity IS. A future ruleset may move the
   138 | /// default; moving what testnet-11's genesis froze would be a different network.
   139 | pub const PALW_RC_COURT_MAX_CLOSE_BYTES: u64 = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
   140 | /// See [`PALW_RC_COURT_MAX_CLOSE_BYTES`]. The floor's widest step recomputes 32,768
   141 | /// multiply-accumulates; this is 512 times that.
   142 | pub const PALW_RC_COURT_MAX_TERMINAL_MACS: u64 = crate::palw_mode_v2::DEFAULT_MAX_TERMINAL_MACS;
   143 | /// See [`PALW_RC_COURT_MAX_CLOSE_BYTES`]. The floor's widest step reads two operands; a
   144 | /// gated-delta-net recurrence reads five.
   145 | pub const PALW_RC_COURT_MAX_OPERAND_COUNT: u32 = crate::palw_mode_v2::DEFAULT_MAX_OPERAND_COUNT;
   146 | 
```

<a id="v2-s10"></a>

## V2-S10 — Final と bond exit hold の既存試験

`consensus/core/src/palw_tir_public_fold_v1.rs` L1564–L1578  
SHA-256: `9092c26113c4636ee8ea7bbcc3550f1648c0134e7cbeccd59b723d42c20531ef`

```text
  1564 |         let root_before = state.state_root();
  1565 | 
  1566 |         // Before any verdict: the claim cannot reach Final and Gate C waits while the frame's d0 door is open.
  1567 |         let claim = state.claims.get(&claim_id).unwrap().clone();
  1568 |         let floor = palw_claim_final_floor_v1(&state, &params, &claim_id, &claim, terms.accepted_daa + 3).unwrap();
  1569 |         assert!(floor >= terms.collect_until_daa, "no Final (weight, receipt spend, escrow, Gate C credit) before the door closes");
  1570 |         assert!(state.palw_tir_public_prosecution_open_v1(&claim_id, terms.collect_until_daa - 1));
  1571 |         assert!(!state.palw_tir_public_prosecution_open_v1(&claim_id, terms.collect_until_daa));
  1572 |         for liable in std::iter::once(producer).chain(colluders()) {
  1573 |             assert!(state.staged_bond_exit_held_v1(&liable, terms.collect_until_daa - 1), "every liable bond is exit-held");
  1574 |         }
  1575 |         assert!(!state.staged_bond_exit_held_v1(&honest_seat(), terms.accepted_daa), "a drawn seat without a Valid is not liable");
  1576 | 
  1577 |         // Liable parties cannot file; nothing moves.
  1578 |         for liable in std::iter::once(producer).chain(colluders()) {
```

## 外部一次資料（設計上の参照）

- [RFC 2212 — Specification of Guaranteed Quality of Service](https://www.rfc-editor.org/rfc/rfc2212.html): 1997-09。Introduction / End-to-End Behavior / Network Element Data Handling Requirements。帯域予約・流量制御・遅延上限には admission と各経路要素のサービス条件が必要という設計原則のみ参照する。misakas の帯域を保証する資料ではない。

- [OP Stack — Fault Dispute Game](https://specs.optimism.io/fault-proof/stage-one/fault-dispute-game.html): 参照日 2026-10-19。単一 step と認証データ、Game Clock の有界な延長、Finalization と bond distribution の区別を参照する。OP の実装を移植したものではない。

- [Kaspa Research — Pruning safety in the vProgs architecture](https://research.kas.pa/t/pruning-safety-in-the-vprogs-architecture/411): 初回投稿 2025-08-25。研究提案であって、採用済み Kaspa 合意規則ではない。pruned data を後続検証が暗黙に必要としない設計上の論点のみ参照する。本 RFC は同記事の ZK proof 方式を導入しない。

<a id="v2-s11"></a>
## V2-S11 — snapshot全体のcanonical hashと外部snapshotのroot照合

`consensus/core/src/palw_state_v2.rs` L24425–L24444  
SHA-256: `ce6a025a3457b8505b0e328ebd75d447b2397256831916d6d971647b0ffd4fa7`

このdigestの呼出し頻度やnodeの実時間コストは未測定であり、毎ブロックのボトルネックと断定していない。

```text
 24425 |     /// `H(canonical(carriage))` — what a pruning proof commits to when it carries the snapshot.
 24426 |     pub fn digest(&self) -> Hash64 {
 24427 |         let bytes = borsh::to_vec(self).expect("PalwStateCarriageV2 is borsh-serializable");
 24428 |         let mut state = keyed(PALW_STATE_V2_DOMAIN_CARRIAGE);
 24429 |         state.update(&(bytes.len() as u64).to_le_bytes());
 24430 |         state.update(&bytes);
 24431 |         finish(state)
 24432 |     }
 24433 | 
 24434 |     /// Reconstruct the live state: rebuild every index, then refuse the snapshot unless all of
 24435 |     /// its derivable facts agree with its primary data AND its state root matches `expected_root`
 24436 |     /// when one is supplied.
 24437 |     ///
 24438 |     /// `expected_root: None` is for LOCAL, already-trusted snapshots only (a node reloading its
 24439 |     /// own disk). A peer-supplied carriage MUST come with the root the chain committed to:
 24440 |     /// self-consistency alone cannot catch a tamper that adjusts a claim and nothing derivable
 24441 |     /// from it — the claim's `reserved`/`immature_contribution` snapshots are the accounting
 24442 |     /// basis by design, so a coherent lie about `pwu` is coherent. The root is what makes it a
 24443 |     /// lie about a DIFFERENT state; "load then notice later" is how a poisoned summary becomes a
 24444 |     /// sink.
```
