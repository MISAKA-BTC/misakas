# RFC18 shared frontier — source excerpts

- Archive: `misakas-pre (1).zip`
- SHA-256: `0c356b39f40d57d8c4d4d69321f38d38b766865024255f38897bfe8206c3b8b0`
- Scope: latest conversation ZIP; the contents/diff of commit `7345cd32a` were not retrieved.
- Static source inspection only; no Rust or devnet execution in this review.
- Original line numbers are shown below. Proposal descriptions in the response are not implemented by this review.

## F01 — claimのterminal状態とunresolved index

`consensus/core/src/palw_state_v2.rs:4745–4811`

```text
  4745 | #[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
  4746 | pub enum PalwClaimPhaseV2 {
  4747 |     Provisional,
  4748 |     PanelBound {
  4749 |         bound_daa: u64,
  4750 |     },
  4751 |     ReceiptLicensed {
  4752 |         licensed_daa: u64,
  4753 |     },
  4754 |     Final {
  4755 |         final_daa: u64,
  4756 |     },
  4757 |     Voided {
  4758 |         voided_daa: u64,
  4759 |         reason: PalwVoidReasonV2,
  4760 |     },
  4761 |     /// **ADR-0062 SA-3: a data-availability accusation is open on this claim.**
  4762 |     ///
  4763 |     /// The claim is still LIVE — its reservation is held, its escrow is neither paid nor
  4764 |     /// destroyed — and it resolves one of two ways, both arithmetic:
  4765 |     ///
  4766 |     /// * a [`PalwConsensusObjectV2::MaterialDisclosed`] opening the accused index against the
  4767 |     ///   claim's own pinned trace root refutes the accusation, charges the ACCUSER and nobody
  4768 |     ///   else (SA-4), and puts the claim back in `resumed` with the deadline that phase derives;
  4769 |     /// * no such disclosure inside `W_disclose` of the accusation confirms the default, and the
  4770 |     ///   sweep does what today's `ProducerDefaulted` arm does immediately —
  4771 |     ///   `void_and_slash(ProducerWithholding)`, bounded at `claim.reserved` (SA-5).
  4772 |     ///
  4773 |     /// **Absence is read from the FOLD, never from a clock** (SA-3). The deadline is
  4774 |     /// `accused_daa + palw_da_disclose_window_daa_v1(params)`, it is recomputed on every branch by
  4775 |     /// `assert_deadline_consistency`, and the disclosure may ride ANY block — so suppressing it
  4776 |     /// needs every producer for the whole window, and `W_disclose ≥ 2 × the finality window` means
  4777 |     /// a reorg across the deadline cannot flip the verdict without a finality violation.
  4778 |     ///
  4779 |     /// **Appended last, deliberately.** Borsh tags an enum by declaration index, so a variant added
  4780 |     /// at the end leaves every existing claim record byte-identical — a state written before this
  4781 |     /// existed still decodes, and every root over it is unchanged.
  4782 |     DefaultDisputed {
  4783 |         /// The DAA at which the accusation was accepted; the disclose deadline is derived from it
  4784 |         /// so the index stays rebuildable from the claims alone.
  4785 |         accused_daa: u64,
  4786 |         /// The one index this accusation names. An accusation that names nothing cannot be
  4787 |         /// refuted, and one that names everything is not an accusation (Decision 1).
  4788 |         missing_event_index: u32,
  4789 |         /// Who accused, and therefore who pays if the material appears (SA-4).
  4790 |         accuser: PalwBondKeyV2,
  4791 |         /// What the accuser reserved, snapshotted at the accusation — release must return exactly
  4792 |         /// what reserve took, whatever happens to the claim's `reserved` afterwards. This is the
  4793 |         /// same reason `claim.reserved` is snapshotted at creation.
  4794 |         accuser_exposure: u128,
  4795 |         /// The phase the claim was in when the accusation opened, restored on refutation.
  4796 |         ///
  4797 |         /// D4 said "return to `ReceiptLicensed { licensed_daa: the disclosure's DAA }`". That
  4798 |         /// restarts the challenge clock, which (a) punishes a producer the chain has just proven
  4799 |         /// honest and (b) lets serial accusations hold a claim — and its reservation — open
  4800 |         /// forever, one window at a time. Restoring the phase the claim actually held keeps the
  4801 |         /// claim's own clock running underneath the session, so the whole DA session is bounded by
  4802 |         /// the claim's retention window and adds nothing to the lattice (SA-6).
  4803 |         resumed: Box<PalwClaimPhaseV2>,
  4804 |     },
  4805 | }
  4806 | 
  4807 | impl PalwClaimPhaseV2 {
  4808 |     pub fn is_terminal(&self) -> bool {
  4809 |         matches!(self, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. })
  4810 |     }
  4811 | }
```

## F01b — unresolved indexの更新

`consensus/core/src/palw_state_v2/transition_builder.rs:800–810`

```text
   800 |         // The unresolved index tracks non-terminal claims; keep it in lockstep with every write.
   801 |         if let Some(previous) = &old
   802 |             && !previous.phase.is_terminal()
   803 |         {
   804 |             self.state.unresolved.remove(&(previous.accepted_blue_score, key));
   805 |         }
   806 |         if let Some(record) = &new
   807 |             && !record.phase.is_terminal()
   808 |         {
   809 |             self.state.unresolved.insert((record.accepted_blue_score, key));
   810 |         }
```

## F02 — resolved prefixとfrontier再計算

`consensus/core/src/palw_state_v2.rs:15018–15095`

```text
 15018 |     // 5. Frontier observation — the definition `palw_fork_choice` states and this used to miss:
 15019 |     //    **the deepest block on this chain whose PALW work is `Final`, with nothing unresolved
 15020 |     //    below it.**
 15021 |     //
 15022 |     // What stood here asked `unresolved.is_empty()` — whether the chain, tip included, held no
 15023 |     // open claim — and named THIS block. Two failures fell out of it (audit C2), and they pull in
 15024 |     // opposite directions, which is why one rule has to answer both:
 15025 |     //
 15026 |     // * On a chain that produces work the condition is never true. Step 4 above just inserted
 15027 |     //   this block's own `Provisional` claim, so at every block forever the frontier stayed
 15028 |     //   wherever the chain last went idle. `pruning_ceiling_v2` froze with it (the node never
 15029 |     //   prunes again, and the carriage grows one claim per block without bound), and the
 15030 |     //   comparator's first key stopped discriminating between honest chains.
 15031 |     // * On a chain that produces NOTHING the condition is trivially true, so a fork carrying no
 15032 |     //   attempts at all advanced its frontier once per block and OUTRANKED a chain that had
 15033 |     //   matured real work. Reproduced: 60 attempt-less blocks beat a 60-block honest chain on
 15034 |     //   key 1, and the deep-reorg gate allowed it. That is precisely the fabrication that
 15035 |     //   ordering by frontier before weight exists to refuse.
 15036 |     //
 15037 |     // Both close by measuring MATURED WORK rather than the absence of open claims. `unresolved`
 15038 |     // is ordered by `accepted_blue_score`, so the resolved prefix ends immediately below its
 15039 |     // first entry; inside that prefix the frontier is the deepest `Final` claim — a claim, so it
 15040 |     // names its own accepting block, and a chain with no matured work has no frontier to stand
 15041 |     // on however long it grows. `Voided` claims are resolved (they do not hold the prefix back)
 15042 |     // but confer no frontier: a block whose work was thrown out matured nothing.
 15043 |     let old_frontier = (builder.state.safe_frontier_blue_score, builder.state.safe_frontier);
 15044 |     let resolved_through = match builder.state.unresolved.iter().next() {
 15045 |         // Nothing open anywhere: the whole chain including this block is a resolved prefix.
 15046 |         None => u64::MAX,
 15047 |         // Everything strictly below the oldest open claim is resolved. Its own point is not —
 15048 |         // pruning there would delete history a live claim still needs.
 15049 |         Some((oldest_open, _)) => oldest_open.saturating_sub(1),
 15050 |     };
 15051 |     // **RFC-0018 Gate C's frontier site** (adopted 2026-10-15): past the fence the frontier is the deepest SecurityCertified ATTEMPT in
 15052 |     // the resolved prefix — the one consensus-credit rule settlement depth shares (`palw_consensus_credit_anchor_v1`). A `Final`
 15053 |     // claim not (yet) certified is resolved (it holds no prefix back) but matures nothing; a Free-Prompt `Final` never stands as the
 15054 |     // frontier. The identity below its fence (keyed on each claim's acceptance DAA) and on every shipped network.
 15055 |     if let Some(claim) = builder
 15056 |         .state
 15057 |         .claims
 15058 |         .iter()
 15059 |         .filter(|(id, c)| {
 15060 |             matches!(c.phase, PalwClaimPhaseV2::Final { .. })
 15061 |                 && c.accepted_blue_score <= resolved_through
 15062 |                 && crate::palw_rfc0018_v1::palw_gate_c_advances_frontier_v1(builder.params, &builder.state, id, c)
 15063 |         })
 15064 |         .map(|(_, c)| c)
 15065 |         .max_by_key(|c| c.accepted_blue_score)
 15066 |         && claim.accepted_blue_score > old_frontier.0
 15067 |     {
 15068 |         builder.state.safe_frontier_blue_score = claim.accepted_blue_score;
 15069 |         // **This hash is the claim's CARRYING block, which since ADR-0058 need not be on the
 15070 |         // selected chain** (audit M2-27). For a merged work it is the merged block — at
 15071 |         // `ghostdag_k = 1` typically a RED. The pair is still coherent (both halves describe that
 15072 |         // same block), and every consumer today reads only the blue score, which is what fork
 15073 |         // choice compares. What is NOT true any more is the old sentence "the deepest block on this
 15074 |         // chain": a consumer that resolves this hash must not assume chain membership, and the
 15075 |         // field's own definition is corrected to say so rather than the value being changed —
 15076 |         // the carrying block is what the job anchor is derived from, so it is load-bearing here.
 15077 |         builder.state.safe_frontier = claim.accepted_block;
 15078 |     }
 15079 |     // **RFC-0018 §7.5a (Gate C in force at this block): the frontier is RECOMPUTED** — the deepest frontier-eligible Final in the
 15080 |     // resolved prefix, never below the floor (seeded with the frontier above at the first such block, raised by retirements). So a
 15081 |     // certified claim later convicted stops standing as the frontier and the frontier falls back to the deepest remaining eligible
 15082 |     // claim; the delta's `Frontier` entry carries any move, so a reorg reverts it exactly. Below the fence the incremental rule above.
 15083 |     if builder.params.rfc0018_v1().is_some_and(|mirror| mirror.gate_c_at(ctx.daa_score)) {
 15084 |         let seeded = (builder.state.safe_frontier_blue_score, builder.state.safe_frontier);
 15085 |         palw_security_certification_fold_v1::ensure_frontier_floor_v1(&mut builder, ctx, seeded)?;
 15086 |         let (blue_score, block) =
 15087 |             crate::palw_security_certification_v1::palw_gate_c_frontier_v1(&builder.state, builder.params, resolved_through);
 15088 |         builder.state.safe_frontier_blue_score = blue_score;
 15089 |         builder.state.safe_frontier = block;
 15090 |     }
 15091 |     let new_frontier = (builder.state.safe_frontier_blue_score, builder.state.safe_frontier);
 15092 |     debug_assert!(
 15093 |         new_frontier.0 >= old_frontier.0 || builder.params.rfc0018_v1().is_some_and(|mirror| mirror.gate_c_at(ctx.daa_score)),
 15094 |         "the frontier never retreats (except past Gate C, where a conviction moves it back)"
 15095 |     );
```

## F03 — frontier floor、候補選択、rollback bound

`consensus/core/src/palw_security_certification_v1.rs:322–371`

```text
   322 | /// The frontier floor row (`None`: none written — Gate C not yet in force on this chain).
   323 | pub fn palw_certified_frontier_floor_v1(state: &PalwChainStateV2) -> Option<PalwCertifiedFrontierFloorV1> {
   324 |     state
   325 |         .kernel_route_state_v1()
   326 |         .and_then(|route| route.aux_row::<PalwCertifiedFrontierFloorV1>(PALW_SECURITY_CERTIFICATION_TABLE_V1, &certification_frontier_floor_key_v1()))
   327 | }
   328 | 
   329 | /// **The safe frontier past Gate C (RFC-0018 §7.5a), recomputed from the state every block** — the deepest frontier-eligible `Final`
   330 | /// claim at or below `resolved_through` (a SecurityCertified attempt; a claim accepted below the fence by the released rule), never
   331 | /// below the floor. Because it is recomputed, a certified claim later convicted (voided) stops standing as the frontier at once and
   332 | /// the frontier falls back to the deepest remaining eligible claim — or the floor. A pure function of the state: every node, on every
   333 | /// arrival order, after a reorg (the delta's `Frontier` entry restores any pair) and after IBD (the floor row travels in the
   334 | /// carriage), reads one answer.
   335 | pub fn palw_gate_c_frontier_v1(
   336 |     state: &PalwChainStateV2,
   337 |     params: &crate::palw_state_v2::PalwStateParamsV2,
   338 |     resolved_through: u64,
   339 | ) -> (u64, crate::BlockHash) {
   340 |     let floor = palw_certified_frontier_floor_v1(state)
   341 |         .map(|f| (f.blue_score, Hash64::from_bytes(f.block)))
   342 |         .unwrap_or((0, crate::BlockHash::default()));
   343 |     let deepest = state
   344 |         .claims_iter()
   345 |         .filter(|(id, claim)| {
   346 |             matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
   347 |                 && claim.accepted_blue_score <= resolved_through
   348 |                 && crate::palw_rfc0018_v1::palw_gate_c_advances_frontier_v1(params, state, id, claim)
   349 |         })
   350 |         .map(|(_, claim)| (claim.accepted_blue_score, claim.accepted_block))
   351 |         .max_by_key(|(blue_score, _)| *blue_score);
   352 |     match deepest {
   353 |         Some(candidate) if candidate.0 > floor.0 => candidate,
   354 |         _ => floor,
   355 |     }
   356 | }
   357 | 
   358 | /// **The pruning rollback bound past Gate C (RFC-0018 §7.7)**: the lowest frontier a conviction could still move the frontier back to
   359 | /// — the shallowest frontier-eligible `Final` claim the state still holds (each of them is convictable until it retires, and each is a
   360 | /// possible fall-back target). Pruning must stay at or below it, so no point a rollback makes the frontier again has been pruned. `None`:
   361 | /// no such claim (the frontier can fall only to the floor, which never moves back).
   362 | pub fn palw_frontier_rollback_bound_v1(state: &PalwChainStateV2, params: &crate::palw_state_v2::PalwStateParamsV2) -> Option<u64> {
   363 |     state
   364 |         .claims_iter()
   365 |         .filter(|(id, claim)| {
   366 |             matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
   367 |                 && crate::palw_rfc0018_v1::palw_gate_c_advances_frontier_v1(params, state, id, claim)
   368 |         })
   369 |         .map(|(_, claim)| claim.accepted_blue_score)
   370 |         .min()
   371 | }
```

## F04 — Gate Cアンカーとpruning-progress

`consensus/core/src/palw_security_certification_v1.rs:496–528`

```text
   496 | /// **The one consensus-credit rule for frontier AND settlement depth (RFC-0018 §7.5)**: past the fence a claim earns frontier and
   497 | /// settlement credit iff it is a SecurityCertified **attempt** in `Final`. A free-prompt claim's consensus credit is its certified
   498 | /// spends' weight only; it never moves the frontier or the settlement depth (the "only a Free-Prompt Final exists" case: no frontier,
   499 | /// depth 0). Below the fence (`governs == false`) the released rules: the frontier takes a `Final` of any source, settlement attempts.
   500 | pub fn palw_consensus_credit_anchor_v1(governs: bool, certified: bool, source: &PalwClaimSourceV2, phase: &PalwClaimPhaseV2) -> bool {
   501 |     let is_final = matches!(phase, PalwClaimPhaseV2::Final { .. });
   502 |     if !governs {
   503 |         return is_final;
   504 |     }
   505 |     is_final && certified && matches!(source, PalwClaimSourceV2::Attempt)
   506 | }
   507 | 
   508 | /// **The pruning-progress ceiling past the fence (RFC-0018 §7.7)** — independent of security credit, never above history an open
   509 | /// claim or court still needs: one below the oldest of (a) the oldest unresolved claim, (b) the oldest `Final` claim whose
   510 | /// certification is still undecided (its prosecution window and draws still read its history). `None`: nothing holds history, the
   511 | /// caller's own bound (the chain point) applies.
   512 | pub fn palw_pruning_progress_ceiling_v1(state: &PalwChainStateV2) -> Option<u64> {
   513 |     let oldest_unresolved = state.unresolved_first_blue_score_v1();
   514 |     let pending_certification = state.kernel_route_state_v1().and_then(|route| {
   515 |         route
   516 |             .aux
   517 |             .range((PALW_SECURITY_CERTIFICATION_TABLE_V1, Vec::new())..(PALW_SECURITY_CERTIFICATION_TABLE_V1 + 1, Vec::new()))
   518 |             .filter(|((_, key), _)| key.first() == Some(&CERTIFICATION_CLAIM_PREFIX_V1))
   519 |             .filter_map(|(_, bytes)| borsh::from_slice::<PalwClaimCertificationRowV1>(bytes).ok())
   520 |             .filter(|row| row.outcome.is_none())
   521 |             .filter_map(|row| state.claim(&Hash64::from_bytes(row.claim)).map(|claim| claim.accepted_blue_score))
   522 |             .min()
   523 |     });
   524 |     match (oldest_unresolved, pending_certification) {
   525 |         (None, None) => None,
   526 |         (a, b) => Some(a.into_iter().chain(b).min().unwrap_or(0).saturating_sub(1)),
   527 |     }
   528 | }
```

## F05 — frontier floor書込み・認証creditと報酬horizon

`consensus/core/src/palw_security_certification_fold_v1.rs:254–359`

```text
   254 | /// **The frontier floor's two writers** (journaled kernel rows): at the first block Gate C is in force, the floor is seeded with the
   255 | /// frontier the chain stood at (so activation never moves it back); afterwards a frontier-eligible claim that RETIRES raises it to
   256 | /// its own position (a retired claim can no longer be convicted). A no-op below the fence.
   257 | pub(super) fn ensure_frontier_floor_v1(
   258 |     builder: &mut TransitionBuilder<'_>,
   259 |     ctx: &PalwBlockContextV2,
   260 |     frontier: (u64, Hash64),
   261 | ) -> Result<(), PalwStateV2Error> {
   262 |     if !builder.params.rfc0018_v1().is_some_and(|mirror| mirror.gate_c_at(ctx.daa_score)) {
   263 |         return Ok(());
   264 |     }
   265 |     if palw_certified_frontier_floor_v1(&builder.state).is_some() {
   266 |         return Ok(());
   267 |     }
   268 |     super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
   269 |     let floor = PalwCertifiedFrontierFloorV1 { blue_score: frontier.0, block: frontier.1.as_bytes() };
   270 |     write_row(builder, certification_frontier_floor_key_v1(), Some(&floor))?;
   271 |     Ok(())
   272 | }
   273 | 
   274 | /// `retire_claim`'s hook: a retiring frontier-eligible `Final` claim raises the floor (it is past every conviction window).
   275 | pub(super) fn note_retirement_frontier_floor_v1(
   276 |     builder: &mut TransitionBuilder<'_>,
   277 |     claim_id: &Hash64,
   278 |     claim: &PalwClaimStateV2,
   279 | ) -> Result<(), PalwStateV2Error> {
   280 |     let Some(floor) = palw_certified_frontier_floor_v1(&builder.state) else { return Ok(()) };
   281 |     if !matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
   282 |         || !crate::palw_rfc0018_v1::palw_gate_c_advances_frontier_v1(builder.params, &builder.state, claim_id, claim)
   283 |         || claim.accepted_blue_score <= floor.blue_score
   284 |     {
   285 |         return Ok(());
   286 |     }
   287 |     let raised = PalwCertifiedFrontierFloorV1 { blue_score: claim.accepted_blue_score, block: claim.accepted_block.as_bytes() };
   288 |     write_row(builder, certification_frontier_floor_key_v1(), Some(&raised))?;
   289 |     Ok(())
   290 | }
   291 | 
   292 | /// **The consensus credit of a claim that just certified** — exactly the expressions the re-derivations read (consistency, retirement,
   293 | /// reversal), so the state's accumulators and every re-derivation agree: an attempt's Final safe weight (canonical basis, F-W ceiling,
   294 | /// the budget's grant) and one settled anchor; a free-prompt claim's spends so far. Then the held issuance-funded reward is named.
   295 | fn credit_certified_v1(
   296 |     builder: &mut TransitionBuilder<'_>,
   297 |     now: u64,
   298 |     claim_id: &Hash64,
   299 |     claim: &PalwClaimStateV2,
   300 |     row: &PalwClaimCertificationRowV1,
   301 | ) -> Result<(), PalwStateV2Error> {
   302 |     let weight = match &claim.source {
   303 |         PalwClaimSourceV2::Attempt => {
   304 |             let canonical = builder.canonical_claim_weight(claim);
   305 |             let contribution =
   306 |                 palw_claim_safe_contribution_v3(&builder.state.class_shares, claim, builder.uncertified_weightless, canonical);
   307 |             let contribution = crate::palw_weight_cap_v1::palw_weight_final_safe_v1(builder.params, claim, contribution);
   308 |             builder.state.bond_budget_weight_or(claim_id, contribution)
   309 |         }
   310 |         PalwClaimSourceV2::FreePrompt { quanta, spent } => {
   311 |             let per_quantum = palw_fp_spend_weight_v1(&builder.state, claim, *quanta, &builder.fp_pricing());
   312 |             let spent_weight =
   313 |                 per_quantum.checked_mul(spent.len() as u128).ok_or(PalwStateV2Error::Overflow("certified free-prompt weight"))?;
   314 |             builder.state.bond_budget_weight_or(claim_id, spent_weight)
   315 |         }
   316 |     };
   317 |     builder.state.safe_weight = builder.state.safe_weight.checked_add(weight).ok_or(PalwStateV2Error::Overflow("safe_weight"))?;
   318 |     if matches!(claim.source, PalwClaimSourceV2::Attempt) {
   319 |         builder.settle_anchor(now, true)?;
   320 |     }
   321 |     // **G-15 (`palw_reward_liability_horizon_v1`)**: past the fence the held reward is NOT named at certification — it waits in the row
   322 |     // until the claim's liability horizon (`final + window_court`) has passed and is named by the sweep only if the claim is still
   323 |     // certified then (the deferred release in the sweep, which alone names a governed reward and zeroes it, so it is named once even
   324 |     // when certification lands past the horizon). The weight credit above is unchanged. Below the fence: named now.
   325 |     if crate::palw_g14_gates_v1::palw_certified_reward_held_until_v1(builder.params, claim).is_some() {
   326 |         return Ok(());
   327 |     }
   328 |     name_certified_reward_v1(builder, now, claim_id, claim, row)
   329 | }
   330 | 
   331 | /// **G-15: a deferred certified reward at its claim's retirement** — the claim's liability window closes at the EARLIER of
   332 | /// `final + window_court` (the sweep's release) and its retirement (a retired claim can no longer be convicted). Called by the deadline
   333 | /// sweep just before a terminal claim retires: a `Final`, governed, still-certified claim whose held reward has not been named is named
   334 | /// here, once (the row's `held_reward` is zeroed). A no-op below the fence, for a voided claim and for a reward already named.
   335 | pub(super) fn release_deferred_certified_reward_at_retirement_v1(
   336 |     builder: &mut TransitionBuilder<'_>,
   337 |     now: u64,
   338 |     claim_id: &Hash64,
   339 |     claim: &PalwClaimStateV2,
   340 | ) -> Result<(), PalwStateV2Error> {
   341 |     if crate::palw_g14_gates_v1::palw_certified_reward_held_until_v1(builder.params, claim).is_none() {
   342 |         return Ok(());
   343 |     }
   344 |     let key = certification_claim_key_v1(&palw_certification_id_v1(claim_id));
   345 |     let Some(mut row) = row_of::<PalwClaimCertificationRowV1>(&builder.state, &key) else { return Ok(()) };
   346 |     if row.held_reward == 0
   347 |         || !row.outcome.is_some_and(|(outcome, _)| outcome.certifies())
   348 |         || !crate::palw_rfc0018_v1::palw_independently_work_certified_v1(
   349 |             claim_id,
   350 |             claim,
   351 |             &crate::palw_rfc0018_v1::PalwGateCViewV1::of(builder.params, &builder.state),
   352 |         )
   353 |     {
   354 |         return Ok(());
   355 |     }
   356 |     name_certified_reward_v1(builder, now, claim_id, claim, &row)?;
   357 |     row.held_reward = 0;
   358 |     write_row(builder, key, Some(&row))
   359 | }
```

## F06 — 支払いconsumer

`consensus/core/src/palw_security_certification_fold_v1.rs:361–406`

```text
   361 | /// **The held issuance reward named** (the certified claim's coinbase leg): once, through the once-only settlement guard.
   362 | fn name_certified_reward_v1(
   363 |     builder: &mut TransitionBuilder<'_>,
   364 |     now: u64,
   365 |     claim_id: &Hash64,
   366 |     claim: &PalwClaimStateV2,
   367 |     row: &PalwClaimCertificationRowV1,
   368 | ) -> Result<(), PalwStateV2Error> {
   369 |     // RFC-0008 once-only settlement (dormant): the held issuance reward is named once across every consumer. This guards the
   370 |     // PAYMENT only; the weight credit above is Gate C's own (SecurityCertified) and is neither added nor read here.
   371 |     let releases = row.held_reward > 0 && {
   372 |         use crate::palw_settlement_once_v1::{
   373 |             PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
   374 |         };
   375 |         // RFC08-2: the issuance reward drawn at the claim's Final and held for certification — no new R draw; named at once now.
   376 |         let final_daa = match claim.phase {
   377 |             PalwClaimPhaseV2::Final { final_daa } => Some(final_daa),
   378 |             _ => None,
   379 |         };
   380 |         builder
   381 |             .settle_once_liable_v1(
   382 |                 now,
   383 |                 PalwSettlementObligationV1::of(PalwSettlementLegV1::CertificationRelease, claim_id.as_bytes()),
   384 |                 PalwSettlementConsumerV1::SecurityCertification,
   385 |                 row.held_reward,
   386 |                 PalwSettlementDebitV1::Held { amount: row.held_reward },
   387 |                 super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::V2Claim {
   388 |                     claim: *claim_id,
   389 |                     final_daa,
   390 |                     horizon: None,
   391 |                     draw: 0,
   392 |                     paid_now: row.held_reward,
   393 |                     exported_before: None,
   394 |                     vest_until: None,
   395 |                 },
   396 |             )?
   397 |             .pays()
   398 |     };
   399 |     if releases {
   400 |         let key = palw_certification_payout_key_v1(claim_id);
   401 |         let held = builder.state.pending_payouts.get(&key).map(|p| p.amount).unwrap_or(0);
   402 |         let amount = held.checked_add(row.held_reward).ok_or(PalwStateV2Error::Overflow("certified reward"))?;
   403 |         builder.write_payout(key, Some(PalwPayoutV2 { payload: Hash64::from_bytes(row.payout_payload), amount }))?;
   404 |     }
   405 |     Ok(())
   406 | }
```

## F07 — transaction settlement RPCの実際の判定

`consensus/core/src/palw_settlement_v1.rs:19–129`

```text
    19 | use crate::palw_state_v2::{PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwStateParamsV2};
    20 | use kaspa_hashes::ZERO_HASH64;
    21 | 
    22 | /// What the sink's PALW state says about the transactions the selected chain accepted at one DAA
    23 | /// score (ADR-0127 Decision 3).
    24 | #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    25 | pub struct PalwSettlementV1 {
    26 |     /// The DAA score of the chain block the state stands at — the point this answer is true at.
    27 |     pub sink_daa: u64,
    28 |     /// The safe frontier stands at or past the asked DAA score, so everything the selected chain
    29 |     /// accepted there is settled. False on a chain that has no frontier yet.
    30 |     pub settled: bool,
    31 |     /// The settlement depth: attempt claims the state retains in `Final` whose accepting chain block
    32 |     /// is at or after the asked DAA score. Anchors, not blocks.
    33 |     pub depth: u64,
    34 |     /// Attempt claims at or after the asked DAA score still on their way — neither `Final` nor
    35 |     /// `Voided`. A voided claim is no anchor and is in neither count.
    36 |     pub pending: u64,
    37 |     /// Claims at or after the asked DAA score may have retired from the state, so `depth` counts at
    38 |     /// least that many anchors and possibly fewer than there were.
    39 |     pub depth_is_lower_bound: bool,
    40 |     pub safe_frontier_blue_score: u64,
    41 |     /// The DAA score of the chain block that accepted the frontier's claim; 0 with no frontier.
    42 |     pub safe_frontier_daa: u64,
    43 |     /// **RFC-0018 §7.8 (honest reporting past Gate C)**: attempt claims at or after the asked DAA score that are `Final` but NOT
    44 |     /// SecurityCertified — the procedure completed, no consensus or settlement credit. Counted in neither `depth` nor `pending`; 0
    45 |     /// below the fence. A chain whose claims are all uncertified reports `settled == false` and `depth == 0` however many `Final`
    46 |     /// claims it holds: no PALW settlement finality (there is no DNS finality to fall back on — DNS-BFT is abolished, RFC-0012).
    47 |     pub uncertified_final: u64,
    48 |     /// **RFC-0018 §7.2**: Gate C is in force at the sink — every `depth`/`settled` above rests on SecurityCertified claims, a
    49 |     /// PREMISE-BASED certification (the honest fraction of drawn verifier stake is at least the threshold), not an unconditional proof.
    50 |     pub gate_c_in_force: bool,
    51 | }
    52 | 
    53 | /// **The settlement read** (ADR-0127 Decision 3) — `state` is the sink's, `params` the rules it was
    54 | /// folded under, `daa_score` the accepting chain block's DAA score (a UTXO entry's
    55 | /// `block_daa_score`).
    56 | ///
    57 | /// The frontier names the block that CARRIED its claim, which since ADR-0058 may be a merged block
    58 | /// whose own DAA score is below the chain block that accepted it; what orders the chain is the
    59 | /// accepting block. So the frontier is dated by its claim's `accepted_daa` while the state retains
    60 | /// the claim, and by `frontier_header_daa` — the carrying header's DAA score, the conservative
    61 | /// reading — once it has retired. `None` only when a frontier exists and neither date is at hand: a
    62 | /// frontier this node cannot date is an answer it does not have, not an unsettled one.
    63 | pub fn palw_settlement_v1(
    64 |     state: &PalwChainStateV2,
    65 |     params: &PalwStateParamsV2,
    66 |     frontier_header_daa: Option<u64>,
    67 |     daa_score: u64,
    68 | ) -> Option<PalwSettlementV1> {
    69 |     let (safe_frontier_blue_score, frontier) = state.safe_frontier();
    70 |     let has_frontier = frontier != ZERO_HASH64;
    71 |     let (mut depth, mut pending, mut frontier_claim_daa, mut uncertified_final) = (0u64, 0u64, None, 0u64);
    72 |     for (claim_id, claim) in state.claims_iter() {
    73 |         let is_final = matches!(claim.phase, PalwClaimPhaseV2::Final { .. });
    74 |         // The fold sets the frontier from a `Final` claim of any source, so it is dated by one.
    75 |         if has_frontier && is_final && claim.accepted_block == frontier && claim.accepted_blue_score == safe_frontier_blue_score {
    76 |             frontier_claim_daa = Some(claim.accepted_daa);
    77 |         }
    78 |         // An anchor is a block's attempt; a free-prompt commitment licenses work and anchors nothing.
    79 |         let anchors = match claim.source {
    80 |             PalwClaimSourceV2::Attempt => true,
    81 |             PalwClaimSourceV2::FreePrompt { .. } => false,
    82 |         };
    83 |         if !anchors || claim.accepted_daa < daa_score {
    84 |             continue;
    85 |         }
    86 |         // Every phase named, so a phase added to the lattice has to be placed here to compile.
    87 |         // RFC-0018 §7.5: past Gate C (keyed on the claim's acceptance DAA) a Final attempt is an anchor only when SecurityCertified —
    88 |         // the same rule the frontier takes (`palw_consensus_credit_anchor_v1`). Below the fence every Final attempt, as released.
    89 |         let governs = crate::palw_rfc0018_v1::palw_gate_c_governs_v1(params, claim);
    90 |         if governs
    91 |             && matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
    92 |             && !crate::palw_rfc0018_v1::palw_independently_work_certified_v1(
    93 |                 claim_id,
    94 |                 claim,
    95 |                 &crate::palw_rfc0018_v1::PalwGateCViewV1::of(params, state),
    96 |             )
    97 |         {
    98 |             uncertified_final += 1;
    99 |             continue;
   100 |         }
   101 |         match claim.phase {
   102 |             PalwClaimPhaseV2::Final { .. } => depth += 1,
   103 |             PalwClaimPhaseV2::Voided { .. } => {}
   104 |             PalwClaimPhaseV2::Provisional
   105 |             | PalwClaimPhaseV2::PanelBound { .. }
   106 |             | PalwClaimPhaseV2::ReceiptLicensed { .. }
   107 |             | PalwClaimPhaseV2::DefaultDisputed { .. } => pending += 1,
   108 |         }
   109 |     }
   110 |     let safe_frontier_daa = if has_frontier { frontier_claim_daa.or(frontier_header_daa)? } else { 0 };
   111 |     let sink_daa = state.last_point().map_or(0, |point| point.daa_score);
   112 |     // A claim retires at the first block whose DAA score is past `terminal_daa + retirement`, and a
   113 |     // claim accepted at or after `d` went terminal at or after `d`. So none of them can have left
   114 |     // while `sink_daa <= d + retirement`; and none has left at all while no `Final` attempt ever
   115 |     // retired, which `retired_safe_weight` records (every attempt claims at least one pwu).
   116 |     let retirement = params.claim_retirement_daa();
   117 |     let depth_is_lower_bound = retirement > 0 && state.retired_safe_weight() > 0 && sink_daa > daa_score.saturating_add(retirement);
   118 |     Some(PalwSettlementV1 {
   119 |         sink_daa,
   120 |         settled: has_frontier && safe_frontier_daa >= daa_score,
   121 |         depth,
   122 |         pending,
   123 |         depth_is_lower_bound,
   124 |         safe_frontier_blue_score,
   125 |         safe_frontier_daa,
   126 |         uncertified_final,
   127 |         gate_c_in_force: params.rfc0018_v1().is_some_and(|mirror| mirror.gate_c_at(sink_daa)),
   128 |     })
   129 | }
```

## F08 — nodeのpruning上限

`consensus/src/pipeline/virtual_processor/processor/palw.rs:792–840`

```text
   792 |     /// Unit D, site 3: whether a proposed pruning point respects the safe frontier.
   793 |     ///
   794 |     /// History under trial is not prunable — an unresolved claim's history and an open court's
   795 |     /// committed roots are evidence, and pruning them would make a dispute unadjudicable by
   796 |     /// deletion. `true` on a network with no V2 bundle: there is no frontier to respect.
   797 |     pub(crate) fn palw_pruning_point_allowed_v2(&self, candidate_point: BlockHash) -> bool {
   798 |         let Some(state_params) = self.palw_state_params_v2.as_ref() else { return true };
   799 |         let store = self.palw_state_v2_store.read();
   800 |         // **Fail CLOSED on what cannot be read, and OPEN only on what genuinely is not there yet.**
   801 |         //
   802 |         // This was one `.ok().flatten()`, which spells both answers the same way: a `load_tip` that
   803 |         // returns `Err` — a corrupted snapshot, or one written by another schema — became `None`
   804 |         // became "every pruning point allowed", so the gate deleted the history under trial exactly
   805 |         // when it had lost the ability to weigh it. The deep-reorg gate beside it was already fixed
   806 |         // for this shape, and records it: the gate failed open on exactly the candidate it could not
   807 |         // weigh. `Ok(None)` is different and stays open — a V2 chain before its first tip has no
   808 |         // frontier to respect, and refusing there would mean a fresh node could never prune.
   809 |         let state = match store.load_tip_retained_v1(state_params) {
   810 |             Ok(Some((_, state))) => state,
   811 |             Ok(None) => return true,
   812 |             Err(_) => return false,
   813 |         };
   814 |         let Ok(header) = self.headers_store.get_header(candidate_point) else { return false };
   815 |         // G14 lane closure G-K4 (dormant: rows exist only past the kernel route's fence): a segmented claim's served parts live only
   816 |         // in block bodies, so pruning never passes the first block that served one while that claim is still convictable.
   817 |         if state.kernel_route().and_then(|route| route.served_material_pruning_ceiling_v1()).is_some_and(|c| header.blue_score > c) {
   818 |             return false;
   819 |         }
   820 |         let (frontier, _) = state.safe_frontier();
   821 |         // **RFC-0018 §7.7 (Gate C)**: past the fence the frontier moves only on SecurityCertified claims, so pruning must not wait for
   822 |         // it — the ceiling is the pruning-progress rule, independent of security credit: below the oldest unresolved claim and the
   823 |         // oldest Final claim whose certification is still undecided (history an open claim, court or certification still reads),
   824 |         // never above. Below the fence (every shipped network) the frontier rule, unchanged.
   825 |         let gate_c = state_params.rfc0018_v1().is_some_and(|mirror| {
   826 |             mirror.gate_c_at(state.last_point().map_or(0, |point| point.daa_score))
   827 |         });
   828 |         if gate_c {
   829 |             // Two bounds, both independent of whether anything certifies: the progress ceiling (open claims, courts, undecided
   830 |             // certifications) and the ROLLBACK bound — the lowest frontier a still-open conviction window could move the frontier
   831 |             // back to (§7.5a/§7.7). Pruning never passes a point that may become the frontier again.
   832 |             let progress = kaspa_consensus_core::palw_security_certification_v1::palw_pruning_progress_ceiling_v1(&state);
   833 |             let rollback = kaspa_consensus_core::palw_security_certification_v1::palw_frontier_rollback_bound_v1(&state, state_params);
   834 |             return match (progress, rollback) {
   835 |                 (None, None) => true,
   836 |                 (a, b) => header.blue_score <= a.into_iter().chain(b).min().unwrap_or(u64::MAX),
   837 |             };
   838 |         }
   839 |         kaspa_consensus_core::palw_fork_authority_v2::pruning_point_allowed_v2(header.blue_score, frontier)
   840 |     }
```

## F09 — 基本fork-choice comparator

`consensus/core/src/palw_fork_choice.rs:32–78`

```text
    32 | /// The two weights plus the frontier they were measured against.
    33 | ///
    34 | /// A frontier without weights, or weights without a frontier, is half an answer — and the half that
    35 | /// is missing is the one that decides. Carried together so no caller can compare across mismatched
    36 | /// halves.
    37 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    38 | pub struct PalwCandidateOrderV1 {
    39 |     /// Blue score of the deepest block on this chain whose PALW work is `Final`.
    40 |     ///
    41 |     /// The ordering's first key, ahead of weight. Unproven work alone cannot advance it, but colluding
    42 |     /// producers and Panel seats may mature claims on a private branch. This key is a rule, not a
    43 |     /// proof against that attack; RFC-0012 requires separate adversarial evaluation.
    44 |     pub safe_frontier_blue_score: u64,
    45 |     /// Σ pwu over `Final` blocks.
    46 |     pub safe_weight: u128,
    47 |     /// `safe_weight` + the bounded immature contribution. Always `>= safe_weight`.
    48 |     pub live_total: u128,
    49 |     /// The tie-break of last resort — the candidate's own hash. Present so the order is TOTAL:
    50 |     /// two candidates that agree on everything else must still order deterministically, or two
    51 |     /// nodes with different insertion histories pick different tips.
    52 |     pub candidate: Hash64,
    53 | }
    54 | 
    55 | impl PalwCandidateOrderV1 {
    56 |     /// `live_total` can never be below `safe_weight`; this constructs the pair so it cannot be.
    57 |     ///
    58 |     /// Taking `bounded_immature` and adding rather than taking `live_total` directly is the point:
    59 |     /// a caller that computed live as "immature only" would produce a total that FALLS when a claim
    60 |     /// matures, and a node would reorg away from the chain that just improved. The type refuses to
    61 |     /// represent that.
    62 |     pub fn new(safe_frontier_blue_score: u64, safe_weight: u128, bounded_immature: u128, candidate: Hash64) -> Self {
    63 |         Self { safe_frontier_blue_score, safe_weight, live_total: safe_weight.saturating_add(bounded_immature), candidate }
    64 |     }
    65 | }
    66 | 
    67 | /// **The** PALW fork-choice order. Every chain-selection site calls this one function.
    68 | ///
    69 | /// `Greater` means the first candidate wins. Total on distinct candidates, because the last key is
    70 | /// the candidate hash — without it two chains agreeing on frontier and both weights would compare
    71 | /// `Equal`, and each node would keep whichever it happened to hold.
    72 | pub fn compare_palw_candidates_v1(a: &PalwCandidateOrderV1, b: &PalwCandidateOrderV1) -> Ordering {
    73 |     a.safe_frontier_blue_score
    74 |         .cmp(&b.safe_frontier_blue_score)
    75 |         .then_with(|| a.safe_weight.cmp(&b.safe_weight))
    76 |         .then_with(|| a.live_total.cmp(&b.live_total))
    77 |         .then_with(|| a.candidate.cmp(&b.candidate))
    78 | }
```

## F10 — Rule Eのexclusive recordからのfrontier・weight導出

`consensus/core/src/palw_fork_choice_rule_e_v1.rs:532–613`

```text
   532 | impl PalwRuleESideAccV1 {
   533 |     /// `common_capital`: `Some(capital in the common past)` for a common bond, `None` otherwise (v1 reads only `is_some`).
   534 |     fn push(&mut self, record: &PalwRuleEClaimRecordV1, common_capital: Option<u64>) -> Result<(), PalwRuleEErrorV1> {
   535 |         self.count = self.count.saturating_add(1);
   536 |         // RFC-0018 Gate C: an uncertified claim is no participation (v1 and v2) — the bond enters the set only after this check.
   537 |         let counted = record.certification != PalwRuleECertificationV1::Uncertified;
   538 |         if record.attempt && counted && common_capital.is_some() {
   539 |             self.bonds.insert(record.bond);
   540 |         }
   541 |         // RFC-0018 §6 (v2): a voided or uncertified attempt does not count; the check precedes the insertion.
   542 |         self.capital_v2.push(record.bond, record.attempt, counted && record.status != PalwRuleEClaimStatusV1::Voided, common_capital);
   543 |         // RFC-0018 §7.5's one rule: past Gate C only a certified ATTEMPT stands as the frontier (a certified free-prompt `Final` keeps
   544 |         // its spends' weight, an uncertified `Final` is resolved and stands as nothing); below it any `Final`, as released.
   545 |         let frontier_ok = match record.certification {
   546 |             PalwRuleECertificationV1::NotGoverned => true,
   547 |             PalwRuleECertificationV1::Certified => record.attempt,
   548 |             PalwRuleECertificationV1::Uncertified => false,
   549 |         };
   550 |         match record.status {
   551 |             PalwRuleEClaimStatusV1::Final => {
   552 |                 if frontier_ok {
   553 |                     self.finals.insert(record.accepted_blue_score);
   554 |                 }
   555 |             }
   556 |             PalwRuleEClaimStatusV1::Live => {
   557 |                 self.oldest_live = Some(self.oldest_live.map_or(record.accepted_blue_score, |o| o.min(record.accepted_blue_score)));
   558 |             }
   559 |             PalwRuleEClaimStatusV1::Voided => return Ok(()),
   560 |         }
   561 |         let add = |sum: &mut u128, weight: u128| -> Result<(), PalwRuleEErrorV1> {
   562 |             *sum = sum.checked_add(weight).ok_or(PalwRuleEErrorV1::Overflow)?;
   563 |             Ok(())
   564 |         };
   565 |         let is_final = record.status == PalwRuleEClaimStatusV1::Final;
   566 |         if let Some(terms) = record.budget {
   567 |             let group = self.budgeted.entry((record.bond, terms.version, terms.window)).or_insert((0, 0, terms.final_weight_ceiling));
   568 |             group.2 = group.2.min(terms.final_weight_ceiling);
   569 |             return if is_final { add(&mut group.0, record.safe_weight) } else { add(&mut group.1, record.live_weight) };
   570 |         }
   571 |         if is_final {
   572 |             add(&mut self.safe, record.safe_weight)
   573 |         } else if record.capped {
   574 |             let entry = self.staged.entry(record.bond).or_insert((0, record.bond_collateral));
   575 |             add(&mut entry.0, record.live_weight)
   576 |         } else {
   577 |             add(&mut self.immature, record.live_weight)
   578 |         }
   579 |     }
   580 | 
   581 |     fn finish(self) -> Result<PalwRuleESideV1, PalwRuleEErrorV1> {
   582 |         let (mut safe, mut immature) = (self.safe, self.immature);
   583 |         for (sum, collateral) in self.staged.values() {
   584 |             immature = immature
   585 |                 .checked_add((*sum).min(collateral.map(palw_bond_weight_cap_v1).unwrap_or(0)))
   586 |                 .ok_or(PalwRuleEErrorV1::Overflow)?;
   587 |         }
   588 |         for (finals, provisional, ceiling) in self.budgeted.values() {
   589 |             let credited_safe = (*finals).min(*ceiling);
   590 |             let credited_total = finals.saturating_add(*provisional).min(*ceiling);
   591 |             safe = safe.checked_add(credited_safe).ok_or(PalwRuleEErrorV1::Overflow)?;
   592 |             immature = immature.checked_add(credited_total - credited_safe).ok_or(PalwRuleEErrorV1::Overflow)?;
   593 |         }
   594 |         let resolved_through = self.oldest_live.map(|oldest| oldest.saturating_sub(1)).unwrap_or(u64::MAX);
   595 |         Ok(PalwRuleESideV1 {
   596 |             participation: u32::try_from(self.bonds.len()).unwrap_or(u32::MAX),
   597 |             safe_frontier_blue_score: self.finals.range(..=resolved_through).next_back().copied().unwrap_or(0),
   598 |             safe_weight: safe,
   599 |             live_total: safe.checked_add(immature).ok_or(PalwRuleEErrorV1::Overflow)?,
   600 |             exclusive_claims: u32::try_from(self.count).unwrap_or(u32::MAX),
   601 |             participation_capital_v2: self.capital_v2.capital(),
   602 |         })
   603 |     }
   604 | }
   605 | 
   606 | /// **One side of rule E from its exclusive records** — the accumulator every side goes through, by the node (streamed from its
   607 | /// state, [`PalwChainStateV2::palw_rule_e_side_v1`]) and by a header-verified client alike. `common_bond` says whether a bond
   608 | /// counts toward participation (registered in the common past). Participation: the distinct common bonds of attempt records —
   609 | /// one per bond, however many records it has; safe weight: the `Final` records' weights; live total: safe plus the uncapped live
   610 | /// weights plus, per bond, F-W's `min(Σ staged, W_cap(collateral))` over its capped live records; records an allocation governs
   611 | /// (ADR-0176 D3): per `(bond, version, window)`, `min(Σ final, F_max)` to the safe weight and `min(Σ final + Σ provisional,
   612 | /// F_max)` to the live total; the frontier: the deepest `Final` record below the oldest live one (the fold's resolved-prefix rule
   613 | /// restricted to the records).
```

## F11 — native settlementのfrontier coverage

`consensus/src/pipeline/virtual_processor/native_settlement.rs:531–574`

```text
   531 |         #[cfg(test)]
   532 |         {
   533 |             for row in rows.iter().filter(|r| r.hash != pruning) {
   534 |                 facts.extend(injected_facts.get(&row.hash).into_iter().flatten().copied());
   535 |             }
   536 |         }
   537 | 
   538 |         // 6. The lifecycle question, once: an effect at `blue` is closed iff every claim accepted at or before it is
   539 |         // resolved (Voided, or Final with its trace retention lapsed) and no dispute session names a claim at or before it
   540 |         // (a session whose claim is gone is open for every effect). Claims already retired are absent from state and
   541 |         // therefore closed.
   542 |         let open_from = native_open_from_v1(
   543 |             state.claims_iter().map(|(id, c)| {
   544 |                 (c.accepted_blue_score, c.phase.clone(), native_effective_retention_v1(&state, params, id, c).unwrap_or(u64::MAX))
   545 |             }),
   546 |             open_sessions.iter().map(|(claim, _)| state.claim(claim).map(|c| c.accepted_blue_score)),
   547 |             sink_daa,
   548 |         );
   549 | 
   550 |         // 7. The longest contiguous certified prefix, oldest first.
   551 |         let effects: Vec<NativeEffectV1> = rows
   552 |             .iter()
   553 |             .rev()
   554 |             .map(|row| NativeEffectV1 {
   555 |                 daa: row.daa,
   556 |                 blue: row.blue,
   557 |                 frontier_covers: frontier_on_branch && frontier_blue >= row.blue,
   558 |                 lifecycle_closed: row.blue < open_from,
   559 |             })
   560 |             .collect();
   561 |         let prefix = certify_native_prefix_v1(retirement.settlement, &effects, sink_daa, &facts);
   562 |         result.stop = prefix.stop;
   563 |         if let Some(index) = prefix.safe {
   564 |             result.safe = Some(rows[rows.len() - 1 - index].hash);
   565 |             result.depth = prefix.evidence.depth;
   566 |             result.unique_work = prefix.evidence.work.to_string();
   567 |         }
   568 | 
   569 |         // 8. Pruning is a checkpoint safety condition, not a replacement for the PALW certificate.
   570 |         if result.safe.is_some_and(|safe| self.reachability_service.try_is_chain_ancestor_of(pruning, safe).unwrap_or(false))
   571 |             && self.evm_header_store.has(pruning).unwrap_or(false)
   572 |         {
   573 |             result.finalized = Some(pruning);
   574 |         }
```

## F12 — RFC18の認証、frontier、pruning、settlement規則

`docs/rfc/0018-dag-parallel-multi-model-consensus.md:192–268`

```text
   192 | 
   193 | ### 7.1 二つの状態
   194 | 
   195 | | 状態 | 意味 | 何が起きるか |
   196 | | --- | --- | --- |
   197 | | `Final` | claimの手続きが完了 | 利用者払いescrowの経済的settlementは既存規則どおり |
   198 | | `SecurityCertified` | 合意creditを持ってよい | safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来(coinbase)報酬legに寄与 |
   199 | 
   200 | 不変条件(fence以後、claimの受理DAAで判定):
   201 | 
   202 | * **I-C1:** `SecurityCertified`でないclaimは、上の7つのどれにも寄与しない(MUST NOT)。
   203 | * **I-C2:** `Final`、Panelのlicence、Panelの過半、モデル保有者の集合、登録者が満たせる集合のいずれも、単独で認証にならない(MUST NOT)。
   204 | * **I-C3:** 認証済みclaimが後でG14の公開告発で反証されたら、voidされ、合意creditは取り消される(MUST)。safe weightは`reverse_convicted_final`で引かれ、participation・settlement depthから消える。
   205 | * **I-C4:** DNS-BFTおよびDNS finality anchorに安全性・収束を依存しない(MUST NOT)。
   206 | * **I-C5:** 何も認証されない間も、DAG、通常取引、heartbeat/BASE-0の時計、DAAは進む。止まるのはfinality creditだけ(MUST)。
   207 | 
   208 | ### 7.2 認証述語(前提に基づく認証)
   209 | 
   210 | 1. **commit:** 支配されたclaimの`Final`で、claim・producer・母集団の切断点を固定する(`commit_at_final_v1`、`consensus/core/src/palw_security_certification_fold_v1.rs:45`)。
   211 | 2. **draw:** commitより厳密に後のexecution laneのseed anchor(ADR-0130)から作るseedで、**global pool**(Active、commitの`verifier_maturity`前に登録、最小担保以上、producerのoperatorを除く)から`m`位置を担保比例・復元抽選する(`palw_certification_pool_admits_v1`、`palw_security_certification_v1.rs:280`、RFC-0017/R7-ECONの抽選を再利用)。**モデル保有の有無で母集団を絞らない**。
   212 | 3. **attest:** 抽選されたbondは期限内に、そのclaimを再実行して判定を署名する(RFC-0017のinner 75を、exam idの代わりにcertification idで使う、`try_attest_claim_certification_v1`、fold `:111`)。
   213 | 4. **decide:** `k`位置以上が`Reproduced`(mismatchなし)で、claimがまだ`Final`、未解決の告発がなく、**G14公開告発window**(`prosecution_window_daa`)が`Final`から経過したら`Certified`(`palw_certification_verdict_v1`、`:320`)。期限までに`k`未満なら`NotCertified`、母集団不足は`NoPopulation`、`certify_by`までに抽選できなければ`Expired`、`Final`でなくなれば`Withdrawn`。**1 claimにつき1回、再試行なし、classごとに数える**(attemptの有界化と計数)。network全体のepoch当たり抽選数にも上限がある。
   214 | 5. **公開check経路:** 決定論的な公開検査(G14完了classの公開material)が固定されたclassは、告発window経過で`PublicCheckCertified`。このbinaryでは固定classは0件(`PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1 = &[]`)。
   215 | 
   216 | **前提(明示):** 認証は抽選検証者の署名付き承認に依る。**「抽選されたverifier stakeのうち正直な割合が閾値(`k/m`)以上である」ことを前提とする。** 署名は再計算の証明ではない。これは無条件の数学的証明ではなく、前提に基づく認証である。RPCはこれを`securityCertificationPremise`として返す(§7.8)。
   217 | 
   218 | 閉鎖モデルは、正直な抽選検証者が入手も再実行もできないので、正直な位置から肯定承認を得られない。攻撃者が`m`位置中`k`以上を引いたときだけ認証される。
   219 | 
   220 | ### 7.3 前提が崩れた場合の影響(報告値、設計の変更条件ではない)
   221 | 
   222 | 攻撃者のverifier stake比`f`(抽選される母集団に占める割合)ごとの、閉鎖モデルの1 claimが認証される確率`P(X ≥ k)`、`X ~ Bin(m, f)`(`palw_certification_false_pass_f64`):
   223 | 
   224 | | `f` | 7-of-5 | 47-of-32(設計値) | 23,076 claimでの期待認証数(47/32) |
   225 | | --- | --- | --- | --- |
   226 | | 0.03 | 4.85e-7 | 8.9e-38 | ≈ 0 |
   227 | | 0.25 | 1.29e-2 | 6.4e-10 | 1.5e-5 |
   228 | | 1/3 | 4.53e-2 | 1.19e-6 | 0.027 |
   229 | | 0.6 | 0.420 | 0.163 | ≈ 3,762 |
   230 | | 1(最悪) | 1 | 1 | 23,076 |
   231 | 
   232 | * 7-of-5では`f = 1/3`で23,076 claimのうち約1,045件が認証される。claim数に比例して前提失敗の影響が積み上がるので、設計値は`m = 47, k = 32`とした(未承認、U-5)。費用は認証されるclaimごとに`m`回の再実行である。
   233 | * `f = 0.6`や最悪ケースでは、本設計は合意creditを守れない。これは前提の外側であり、設計を変える条件ではない(ユーザー方針2)。
   234 | * veto(否定`v`位置で拒否)は追加案U-6であり、採用設計ではない(設計値`v = 0`)。
   235 | 
   236 | ### 7.4 合意creditの加算点(一箇所)
   237 | 
   238 | 認証が決まったとき、sweep(`sweep_security_certification_v1`、fold `:234`、kernel routeの終了tick `palw_kernel_route_fold_v1.rs:1671`)が、再導出と同じ式でcreditを加える(`credit_certified_v1`、`:196`): attemptはFinalのsafe weight(canonical基準、F-Wの上限、予算の付与)とsettled anchor 1件、free-promptはそれまでのspendのweight。発行由来の報酬legは`Final`で保留され(`hold_issuance_reward_at_final_v1`、`:91`)、認証時に支払われ、失敗時は名付けられない(焼却、新規発行なし)。
   239 | 
   240 | ### 7.5 frontierとsettlementの単一規則
   241 | 
   242 | fence以後、frontierとsettlement depthの両方で、合意creditを生むのは**認証済みattemptの`Final`だけ**とする(`palw_consensus_credit_anchor_v1`、`palw_security_certification_v1.rs:367`)。free-promptの合意creditは認証後のspendのweightだけで、frontierもsettlement depthも動かさない。「free-promptの`Final`だけが存在する」場合は、frontierなし・depth 0(F-5の解消、T-0c)。fence未満は従来(frontierは任意source、settlementはattempt)。
   243 | 
   244 | ### 7.5a frontierの巻き戻し(conviction後の再計算)
   245 | 
   246 | fence以後、safe frontierは毎block**再計算**する: 解決済みprefix内の、frontier資格のある`Final`の最深claim(認証済みattempt。fence未満で受理されたclaimは従来規則)、ただし**floor**を下回らない(`palw_gate_c_frontier_v1`)。
   247 | 
   248 | * **floor:** fenceが効いた最初のblockで、そのときのfrontierで初期化する(有効化でfrontierが戻らない)。資格のある`Final`のclaimが**退役**したとき、その位置まで上げる。退役したclaimはもうconvictされない(`reverse_convicted_final`は退役済みを扱わない)。kernel-route aux 136の`'F'` row(`note_retirement_frontier_floor_v1`、`ensure_frontier_floor_v1`)。
   249 | * **巻き戻し:** 認証済みclaimが後でconvict(void)されると、そのclaimはfrontierでなくなり、frontierは残る資格claimの最深、またはfloorへ戻る。deltaの`Frontier`エントリが任意の移動を記録するので、reorgは正確に戻し、IBDはcarriageでfloor rowを受け取る。
   250 | * **pruning:** pruning上限は、進行の上限(§7.7)と**巻き戻し上限**(convictionで再びfrontierになり得る最も浅い位置 = stateに残る資格`Final`の最小受理blue score、`palw_frontier_rollback_bound_v1`)の小さい方を超えない。再びfrontierになり得る点をpruneすることはない。資格claimは退役で必ずstateから消えるので、上限は有界に進む。
   251 | 
   252 | ### 7.6 寿命
   253 | 
   254 | 認証rowはclaimの退役まで残り、退役後にsweepで消える。`claim_retirement_daa ≥ lifetime_daa()`(`certify_by + attest window`と告発windowの大きい方)でなければならない(MUST、activation checklist)。満たさないと`Final`のclaimが未認証のまま退役する(creditなしで整合はするが、正直なclaimも認証されない)。
   255 | 
   256 | ### 7.7 pruning progress
   257 | 
   258 | fence以後、frontierは認証済みclaimでしか動かないので、pruningはfrontierを待たない。上限は、最古の未解決claimと、認証が未決の最古の`Final` claimの、受理blue scoreの小さい方の1つ下(`palw_pruning_progress_ceiling_v1`、`:379`、processor `palw.rs:719-750`)。open claim・court・未決の認証が読む履歴は消さない(MUST)。未決の認証は`lifetime_daa`で必ず決まるので、pruningの停止は有界である。
   259 | 
   260 | ### 7.8 settlementの意味とRPC
   261 | 
   262 | DNS-BFTがないので、settlementの`safe`/`settled`はPALWの認証済みcreditだけを意味する。未認証claimだけのchainは**PALW settlement finalityなし**(`settled = false`、`depth = 0`)と報告し、`uncertifiedFinalAnchors`(認証されていない`Final` attemptの数)と`securityCertificationPremise`(前提の文言)を返す(`GetPalwSettlementResponse`、wire version 4、gRPC field 13/14)。claimごとの認証状態は`GetPalwClaims`のrow(`securityCertification`、wire version 6、gRPC field 39)で返す。
   263 | 
   264 | **leaf-v2 client経路:** rule Eのrecord(`PalwRuleEClaimRecordV1`)は真のphaseと**認証状態**(`certification`: NotGoverned / Certified / Uncertified)を持ち、leaf-v2 windowにcommitされる。Gate Cの適用(未認証はparticipationなし・frontierなし、FPの`Final`はfrontierなし)はnodeとclientが共有するaccumulatorで行う。clientは`palw_rule_e_certification_summary_v1`で、未認証`Final`の数と前提の文言を得る(`palw_fork_choice_rule_e_leaf_v2.rs`)。
   265 | 
   266 | **header commitment(GAP-R4、2026-10-15に解消):** rule E かつ(rule E v2 または Gate C)が state の点で効くとき、header は **rule-E leaf v3** を commit する(`palw_fork_choice_rule_e_leaf_v3.rs`): window の record を認証状態込みで、registry を `(bond, collateral)` で、registry の資本合計を leaf に。どの文法かは読み手自身の ruleset が点で決める(`palw_rule_e_commit_form_v1`: flat / v1 / rule-E v2 / rule-E v3)。node は `palw_committed_state_root_v2` を commit し、v39 anchor・L2 client(`verify_committed_opening_v2`)・carriage RPC は同じ `palw_verify_committed_leaf_v2` で開く。v3 の fence 未満では v1/v2 の envelope は byte 単位で不変(leaf v2 の window leaf は認証状態の field を含まない)。native settlement の roots chain は、rule E 下では各 row の post-state から commit root を計算する(rule-E leaf は delta から逆算できないため)。RFC-0012のnative settlementも、fence以後は`Final`遷移ではなく認証遷移を成熟workの証拠とする(`palw_native_settlement_v1.rs:161`、`:338`、`:404`)。
   267 | 
   268 | ### 7.9 ADR-0177の変更(明示)
```


## F13 — native settlementもfrontier coverageを要する

`consensus/core/src/palw_native_settlement_v1.rs:609–646`

```text
   609 | /// root verification, frontier coverage and the absence of unresolved lifecycle obligations.
   610 | /// Heartbeat, floor and DNS weights are deliberately not accepted as evidence by this interface.
   611 | ///
   612 | /// This is the per-effect REFERENCE: O(facts). The processor calls [`certify_native_prefix_v1`], which is tested
   613 | /// equal to calling this once per effect.
   614 | pub fn certify_native_effect_v1(
   615 |     policy: PalwSettlementPolicyV1,
   616 |     effect_order: (u64, u64),
   617 |     snapshot_daa: u64,
   618 |     executed: bool,
   619 |     frontier_covers: bool,
   620 |     lifecycle_closed: bool,
   621 |     history_complete: bool,
   622 |     facts: &[MatureUsefulWorkV1],
   623 | ) -> Result<SettlementEvidenceV1, SettlementStopV1> {
   624 |     use SettlementStopV1::*;
   625 |     let (effect_daa, effect_blue) = effect_order;
   626 |     if !policy.valid() {
   627 |         return Err(InvalidPolicy);
   628 |     }
   629 |     if !history_complete {
   630 |         return Err(MissingHistory);
   631 |     }
   632 |     if !executed {
   633 |         return Err(Unexecuted);
   634 |     }
   635 |     if !frontier_covers {
   636 |         return Err(FrontierNotCovered);
   637 |     }
   638 |     if !lifecycle_closed {
   639 |         return Err(OpenLifecycle);
   640 |     }
   641 |     let mut acc = EvidenceAccumulatorV1::default();
   642 |     let mut anchors = BTreeSet::new();
   643 |     for f in facts
   644 |         .iter()
   645 |         .filter(|f| f.accepted_daa >= effect_daa && f.accepted_blue >= effect_blue && f.matured_daa <= snapshot_daa && f.work > 0)
   646 |     {
```

## F13b — native settlementもfrontier coverageを要する

`consensus/core/src/palw_native_settlement_v1.rs:751–785`

```text
   751 |                 *first = (*first).max(qa);
   752 |             }
   753 |         }
   754 |         let mut anchor_at: Vec<u64> = vec![0; n];
   755 |         for first in anchor_first.values() {
   756 |             anchor_at[*first] += 1;
   757 |         }
   758 |         let mut acc = EvidenceAccumulatorV1::default();
   759 |         results.resize(n, Err(InvalidPolicy));
   760 |         for i in (0..n).rev() {
   761 |             for k in &fact_at[i] {
   762 |                 acc.add_fact(live[*k]);
   763 |             }
   764 |             for _ in 0..anchor_at[i] {
   765 |                 acc.add_anchor();
   766 |             }
   767 |             let e = effects[i];
   768 |             results[i] = if !e.frontier_covers {
   769 |                 Err(FrontierNotCovered)
   770 |             } else if !e.lifecycle_closed {
   771 |                 Err(OpenLifecycle)
   772 |             } else {
   773 |                 acc.evidence(policy)
   774 |             };
   775 |         }
   776 |     }
   777 |     let mut out = NativePrefixV1 { safe: None, evidence: SettlementEvidenceV1::default(), stop: None };
   778 |     for (i, r) in results.into_iter().enumerate() {
   779 |         match r {
   780 |             Ok(e) => {
   781 |                 out.safe = Some(i);
   782 |                 out.evidence = e;
   783 |             }
   784 |             Err(stop) => {
   785 |                 out.stop = Some(stop);
```
