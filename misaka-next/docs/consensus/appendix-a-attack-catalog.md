# Appendix A — Attack catalog

Every attack, exploit, forgery, grind, denial of service and economic hole that was found against
Misaka Proof-of-LLM (PALW) before and during testnet-12 (t12), deduplicated, named, and given a
status. It is the historical input to [10-attack-model.md](10-attack-model.md): the synthesizer
curates the consensus-level entries from here into the normative attack model, and misaka-next's
adversarial simulator must reproduce every name in [§5](#5-consensus-level-regression-list).

## 1. How to read this catalog

**Names are permanent test names.** Each entry's `name` is snake_case and is used verbatim as the
regression test for it: `fn <name>()` in `tests/adversarial/<class>.rs` (simulator scenario
`adversarial::<class>::<name>`). A name is never reused and never renamed; an entry that turns out to
be two holes is split into two new names and the old one is kept as an alias in this appendix. Where an
entry proves an invariant of [09-invariants.md](09-invariants.md), §5 names the INV ID; the invariant
test (`inv_…`) and the attack test are separate tests.

**Deduplication.** One hole found several times is one entry with several sources. Holes with the same
mechanism but a different payoff (for example "grind fake roots to win a lottery" versus "grind fake
roots to tighten `bits`") are separate entries.

**Columns.**

- **sev** — severity as the finding source rated it (critical / high / medium / low); where sources
  disagreed, the adjudicated rating.
- **C** — `Y` if the attack is consensus-level (fork choice, state transition, admission, court,
  clocks, issuance: it belongs in next's consensus model and simulator); `N` if it is node, network,
  wallet or EVM level (a later milestone, listed so it is not lost).
- **t12 status** — at the frozen reference `rcore/int-3 @ a0af3c92` (see
  [PROVENANCE.md](../../PROVENANCE.md)):
  `FIXED` (a rule in the reference closes it; the fix is named), `OPEN` (still reachable in the
  reference, possibly partially mitigated — the mitigation is named), `BY-DESIGN` (accepted residual or
  a design choice, with the bound that makes it acceptable), `PENDING` (closed only on a branch not yet
  in the reference: `feat/t12-aheld-node`, `feat/t12-activation-pool`,
  `feat/t12-class-verify-deadline`, `rcore/p2-file`), `UNKNOWN` (no source settles it). A status is
  *what the sources say*; the chapter authors (01–08) add code-level verdicts. Where this appendix
  states t12 behaviour itself, it cites `path:line` at a0af3c92 that was read for this appendix.
- **sources** — short keys (table below), memory files as `mem:<name>` (the project's institutional
  memory, `~/.claude/projects/-Users-wata-Downloads-MISAKA-testnet/memory/<name>.md`; leads, not
  proof), test files as `T:<path>` (repo-relative at a0af3c92, `…` = `consensus/core/tests`), commits
  as 8-hex shas.

| key | source |
| --- | --- |
| TM | `docs/palw-rc-threat-model.md` (PALW-RC red-test register, P0-1…P0-11, C1…C5, H1…H3, L1…L2) |
| CA0819 | `docs/palw-critical-audit-2026-08-19-ja.md` (attacks A–D, :416-450) |
| A0817 | `docs/palw-only-v4-audit-2026-08-17-ja.md` |
| X0821 | `docs/palw-external-audit-2026-08-21.md` |
| R0822 | `docs/palw-mainnet-readiness-audit-2026-08-22.md` (items 1–11) |
| M0828 | `docs/palw-mainnet-audit-2026-08-28.md` (M1-1…M1-6, M2-1…M2-28) |
| M0829 | `docs/palw-mainnet-audit3-2026-08-29.md` (S-01…S-21) |
| RA0829 | `docs/palw-mainnet-reaudit-2026-08-29.md` (R-1…R-8) |
| M0830 | `docs/palw-mainnet-audit-2026-08-30.md` (union audit + two addenda) |
| M0905 | `docs/palw-mainnet-audit-2026-09-05.md` (O-1…O-18) |
| M0906 | `docs/palw-mainnet-audit-2026-09-06.md` (C-1…C-5, H-1…H-5, M-1…M-16) |
| A0918 | `docs/palw-audit-2026-09-18-6001.md` (C-1, C-2, H-1…H-6, A-04…B-08) |
| DAA0918 | `docs/palw-daa-clock-audit-2026-09-18.md` |
| RW0919 / RW0920 | `docs/adr/STATUS-AUDIT-2026-09-19-llm-mining-reward.md` (F1…F5) / `…-2026-09-20-reward-reaudit.md` |
| AUD0923 | the 2026-09-23 economic audit: `mem:t12-audit-2026-09-23-fence-and-unit-recovery`, `T:consensus/core/tests/audit_*.rs`, fix `b5c5f324` |
| DOS0924 | the 2026-09-24 DoS audit (#1–#14): `T:consensus/core/tests/dos_*.rs`, `review*.rs`, fixes `6bb8c844 b38356fe 8e28aa17 139c9215 d0542304 e5f247fe` |
| HB0924 | the 2026-09-24 heartbeat red-team lanes H1–H3: `T:consensus/core/tests/hb_*.rs`, fixes `43b0b7c9 33d664ef b6f6c545 30de9358` |
| R152 | ADR-0152 reviews (X1…X13, F1…F4, M1…M3, S-4, A-held F2…F8): `mem:adr0152-v2-user-decisions-f1-f4-gate`, `mem:claim-collateral-design-bar`, `mem:held-attention-attribution-8k-gate-2m-cap`, `mem:rcore-int2-integration-0924` |

## 2. The five seed attacks

The project owner's pre-t12 investigation named five attacks. Their verdicts from the sources, with the
t12 lines read for this appendix:

| seed | source verdict | evidence |
| --- | --- | --- |
| `private_fake_root_burst` | **partial** — the fake claim cannot reach `Final`, but the lottery is still ground on free roots and the claim carries bounded live weight before any panel | Roots are inside the lottery preimage (`consensus/core/src/palw_attempt_v2.rs:550-562`) and only replay pins them (`consensus/core/src/config/params.rs:2457-2465`, "C-1 stays open behind this fence"). An accepted claim adds `⌊β·pwu/1000⌋` to `bounded_immature` at acceptance (`consensus/core/src/palw_state_v2.rs:57-61`, `:2035-2036`, `:18650-18654`), which is fork choice's third key (`consensus/core/src/palw_fork_choice.rs:72-78`). The t12 2M row needs a single draw, so any ticket wins with one BLAKE2b (`T:consensus/core/tests/dos_repro_3_one_junk_2m_claim_kills_the_2m_lane.rs` header). Mitigations: staged reserve forfeited, SEAT-0/SEAT-R replay, F1 attribution, 2M closed at launch (pending). |
| `private_daa_finality_acceleration` | **partial** — a branch cannot run the DAA faster than header time, but the fork-choice frontier is measured in blue score, which heartbeats buy | Unpriced lanes advancing the DAA were closed (ADR-0138/0142, `palw_receipt_rows_unpriced` armed on t12 at `consensus/core/src/config/params.rs:15851`); the clock floor bounds beats to one per slot (`params.rs:1305-1322`, armed `:15909`). But key 1 of the comparator is `safe_frontier_blue_score` (`palw_fork_choice.rs:72-78`), set from `claim.accepted_blue_score` (`palw_state_v2.rs:20935`), and K heartbeats then one matured claim outrank 1,000 matured claims (`T:consensus/core/tests/hb_fork_weight.rs:118-176`). ADR-0065 D1 seat maturity is measured on the branch's own anchor DAA. |
| `heartbeat_clock_acceleration` | **closed as acceleration, by design as advancement** — on t12 the heartbeat *is* the DAA clock, rate-limited by wall clock. *Chapter verdict: partial (01 §7; §3.13.2)* | `palw_clock_advances_without_a_claim_v1` is true on t12 (`params.rs:15654-15663`; `T:consensus/core/tests/hb_confirmation_surface.rs:133-148`); the only throttle is the slot cursor (`hb_confirmation_surface.rs:288-307`). Economic liabilities also need the licence-counted second clock (`params.rs:15901`) but it lapses after `2 × window_court` with no licence (`palw_state_v2.rs:2210-2229`). Residuals: DNS veto TTL and quantum maturity (see `dns_veto_expires_on_heartbeat_clock`, `quantum_maturity_reads_wall_clock`). |
| `bond_split_amplification` | **closed** for issuance and fork weight; **bounded** for panel seats. *Chapter verdict: partial — seats, tiers, strikes and per-bond allowances are split-sensitive (02 §6.2; §3.13.2)* | Tickets are per execution, not per bond (ADR-0072). One key one bond (`palw_state_v2.rs:23887`, `DuplicateBondKey`); operator identity proven (`46e84b4d`); the stake-weighted draw weighs posted collateral capped at 1,000,000 MSK per operator (`consensus/core/src/palw_panel_v2.rs:150-176`, `:1643-1652`), so splitting is at most linear in stake. The VLT-era "split your bond, multiply your vote" was real and fixed (`dca5f94`). |
| `failed_lottery_blue_weight` | **real at the GHOSTDAG layer, [unverified] impact on the t12 sink**. *Chapter verdict: real, including the sink — blue work orders the sink heap and every selected parent (06 T12-D5; §3.13.2)* | An attempt block's blue work is a header-only constant `2^20`, assigned whether or not the class lottery was won, because the lottery is checked only in the virtual processor (`consensus/src/processes/ghostdag/protocol.rs:625-634`). A lost-lottery attempt is `StatusDisqualifiedFromChain` but stays in the DAG and, merged blue, pays its work to descendants (M0906 C-1, :128-200). Receipt blocks earn 0 (`protocol.rs:593-610`) and heartbeats ε (`:611-624`). |

## 3. Attacks by class

### 3.1 time — clocks, DAA, deadlines, cadence

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `heartbeat_clock_acceleration` | high | Y | The bondless heartbeat lane (algo 8, `2^24` hashes a beat, ε blue work) advances the DAA clock that deadlines, maturity, withdrawal delays and coinbase maturity read. Variants measured: economic deadlines of 10,500 DAA consumed with zero bond (AUD0923); `Final` counter ticking inside a DAA-driven sweep so heartbeat blocks advanced the "second clock" (DOS0924 finding 9); beats minted between slots (89% of t12's) and a future-stamped sibling pushing the next slot back 132 s (HB0924 H3/H5). | AUD0923; DOS0924 (`T:…/dos_l4_pipeline_anchors.rs`, `dos_l5_5_heartbeat_timeout.rs`); HB0924 (`T:…/hb_liability_expiry.rs`, `hb_confirmation_surface.rs`); `mem:palw-daa-clock-is-the-anchors-clock-adr-0138`; `mem:palw-adr-0142-consensus-clock-cursor` | BY-DESIGN — heartbeat is the whole clock on t12 (ADR-0151 D3); acceleration closed by the cursor + `palw_clock_floor` (`b6f6c545`, `30de9358`); liabilities need licence-counted anchors (`6bb8c844`), escape after `2 × window_court` |
| `unpriced_lane_advances_daa` | critical | Y | A lane that pays no `bits` still advanced the DAA score: the receipt lane (no PoW) let DAA windows run ~180× fast so "courts are won by clock" (M0905 O-7); after the single lottery the attempt lane still counted, so 1 DAA ≈ 12 s and every DAA-denominated window ran 10× fast (DAA0918 §4). | M0905 O-7; DAA0918 §1-§5; `mem:palw-daa-clock-is-the-anchors-clock-adr-0138` | FIXED — ADR-0138 anchor clock (`38f977e2`), `palw_receipt_rows_unpriced` (`params.rs:15851`) |
| `heartbeat_rows_price_bonded_lane_out` | critical | Y | Heartbeat rows sat in the global difficulty window: first their price (`bits`) and later their count tightened `bits` ×3 per window (0x207fffff → p=1.5e-3 in 826 blocks on t11 5f); heartbeats never felt it, bonded lanes could never win again, no self-recovery. | M0830 §heartbeat; `mem:heartbeat-rows-price-attempt-lanes-off-the-chain`; `mem:palw-adr-0066-heartbeat-out-of-bits` | FIXED — ADR-0066 (heartbeat out of `bits`, algo 8), ADR-0083 `palw_difficulty_priced_rows` |
| `bondless_attempt_row_grind` | critical | Y | Header-stage checks of an attempt read no chain state; roots are free, and stepping the nonce by `2^22` yields `2^42` anchors per template. A bondless key grinds attempt headers at ~3 µs a draw; each is disqualified from the chain but stays a `bits`-priced difficulty row, tightening `bits` until bonded producers wait ~139 days (the heartbeat 5f failure re-entered through the attempt lane). | M0906 C-1 (:128-200) | OPEN — `palw_attempt_header_pins` armed on t12 (`params.rs:15853`) pins 3 fields but its own doc says C-1 stays open (`params.rs:2457-2465`); magnitude on t12 depends on whether attempt rows are `bits`-priced there [unverified]. *Chapter verdict: `bits` payoff closed on t12 (§3.13.2)* |
| `clock_slot_rule_freeze` | high | Y | Heartbeat admissibility was `selected_parent.timestamp + interval`; a block that does not advance the clock still moved the next opportunity back, so the DAA froze (drill: DAA 20, zero beats). Exempting heartbeats froze it; counting them unconditionally doubled it. | `mem:palw-adr-0142-consensus-clock-cursor`; DAA0918 §7 | FIXED — ADR-0142 cursor (reference derived, not stored: `ed80c0b0`) |
| `clock_reference_node_local` | high | Y | The heartbeat evidence walk stopped at `Err(get_header)` and the first clock-cursor reference was stored, so archival and pruned nodes gave the same header different verdicts / DAA scores (partition along `--archival`). | M0830; `mem:palw-adr-0066-heartbeat-out-of-bits`; `mem:palw-adr-0142-consensus-clock-cursor` | FIXED — evidence walk deleted (ADR-0066); reference derived from the DAA window (`ed80c0b0`) |
| `economic_deadline_on_heartbeat_clock` | high | Y | Slash liabilities, withdrawal delays and D1 maturity expired on the DAA clock alone, so a heartbeat-only history (no anchors, no bond) released a lying seat's collateral; the first two-clock fix was then broken three ways (retire-before-sign, DAA-ticked counter, exit freeze). | AUD0923; HB0924 H2-1…H2-7 (`T:…/hb_liability_expiry.rs:48-363`); DOS0924 §4 (`T:…/dos_l4_two_clock.rs`) | FIXED — second clock counted on licences, withdrawal gate reads live duty (`6bb8c844`); escape after `2 × window_court` is BY-DESIGN; trickle-freeze fixed (`T:…/review_economic_trickle_freeze.rs`) |
| `quantum_maturity_reads_wall_clock` | medium | Y | Execution-quantum maturity is converted to wall-clock rounds at a fixed 120 s/DAA while the liability it is priced against stays on the DAA clock; a heartbeat miner that simply stops beating slows the DAA and lets rights mature while the liability still runs. The schedule row is also dropped on the DAA clock before the wall-clock maturity. | HB0924 H2-4, H2-5 (`T:…/hb_liability_expiry.rs:230-330`) | UNKNOWN |
| `retarget_ratchet_to_zero` | high | Y | The class retarget expected `share × Σ realized` over the full share table, so idle/frozen classes made producers permanent over-producers: ×4 per boundary to target 0, then `ZeroPreviousTarget` rejects every block. Sibling: the 900/100 lane split not renormalised over producing lanes (×0.9 per epoch while receipts are silent). | TM H1 (:461-485); `mem:palw-audit-2026-09-01-network-kill-panics` | FIXED — normalise over classes that competed, floor 1; lane renormalisation |
| `idle_class_target_relaxation` | medium | Y | Relaxing an idle class's target because nothing was produced lets "waiting buy cadence": silence is not evidence of attempts, and an absolute expectation reintroduced the ratchet. | `mem:palw-adr-0071-hash-coupling-remainder` | FIXED — `converge_idle_target_v1` ceiling |
| `epoch_boundary_budget_mismatch` | medium | Y | Admission read the parent state's budget table against the child's epoch index, so the epoch-crossing chain block could never be a non-floor attempt, and merged non-floor attempts lost their claims permanently. | M0829 S-16; M0906 M-2; `mem:palw-rc-launch-audit-2026-08-21` | FIXED — `palw_epoch_boundary_budget` (`params.rs:15861`) |
| `span_unit_mismatch_short_windows` | high | Y | Registry spans are 600 s but t12 counts a span as 1 DAA, so class receipt windows were 1/5 of intent (2M: 2,799 vs 13,995 DAA) — honest verifiers cannot finish before the deadline. | R152 (`mem:held-attention-attribution-8k-gate-2m-cap`) | PENDING — `palw_class_verify_deadline` on `feat/t12-class-verify-deadline` (`2ce3094a`, `2c4b6516`) |
| `w_controller_counts_nonfinal_blocks` | medium | Y | Past the work target, a model class has no per-epoch block cap and `produced_blocks` counts claims that never finalize, so W can be pushed ×4 an epoch, hardening every honest class. | A0918 A-05 | UNKNOWN. *Chapter verdict: OPEN (partial), §3.13.2* |

### 3.2 claim — admission, identity, one execution one claim

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `private_fake_root_burst` | critical | Y | Grind fabricated execution/trace/output roots (no inference) until one wins the class lottery; the winning fake claim is admitted, reserves collateral and adds bounded immature weight before any panel sees it. On a single-draw class (t12 2M) one hash suffices. ADR-0152 found the root cause: a fake claim is indistinguishable from an honest failure, so a pre-licence reserve is required. | CA0819 attack B; TM P0-10; M0905 O-5; `mem:adr0152-v2-user-decisions-f1-f4-gate` (SEAT-0); `mem:t12-new-collateral-model-0924`; `T:…/dos_repro_3_one_junk_2m_claim_kills_the_2m_lane.rs` | OPEN (partial) — cannot reach `Final`: SEAT-0 `12da66ce`, SEAT-R/S1/S2, F1 attribution; staged reserve forfeited (R-core+); weight still counted (§2); 2M closure PENDING |
| `one_execution_many_claims` | critical | Y | One inference minted many claims: in one block (dedup missing), across blocks (`seen_exec` was a per-block `HashSet`, 8 claims / 25,606 MSK for zero extra work), by re-using a gossiped capture that names its own anchor, or by crediting one committed root once per transaction. | AUD0923 C-1 (`T:…/audit_repro_03_attempt_lane_cross_block_execution_repla.rs`, `audit_c1_replay.rs`); `mem:palw-audit-2026-09-11-fence` B-4 (`9c0a221b`); `mem:palw-audits-2026-08-26-to-30-archive` #3; `mem:palw-mainnet-audit-adr0028` B2 | FIXED — rooted execution index under `palw_audit_2026_09_23` (`b5c5f324`); anchor re-derived from chain-visible inputs |
| `sibling_identity_pow_reuse` | critical | Y | Solve one PoW, then swap commitment fields (trace/output root, bond, retention) or flip signature bytes to mint unlimited sibling block identities under it: the commitment was unbound (P0-1), later only 6 of 14 fields were priced (C4), the algo-7 spend signature was unverified and malleable, and the FP spend envelope had no header-position binding. | TM P0-1, C4; CA0819 attack A; `mem:palw-audits-2026-08-26-to-30-archive` (`276840d0`); `mem:palw-freeprompt-adr-0044` | FIXED — `commitment_root_v2 = H(attempt_id_v2)`, challenge equation in the finalizer arm, exhaustive-destructure test |
| `foreign_bond_signature_theft` | critical | Y | Commit under a victim's bond: the ML-DSA signature was never verified (garbage of the right length passed), or was verified under a key the submitter carried; FP commitments charged any bond. | TM P0-2; R0822 #5 (`cbce765c`); `mem:palw-audits-2026-08-26-to-30-archive` (#1 authority theft) | FIXED — verify under the bond record's key (`82d2db44`; `BondKeyMismatch`) |
| `nonce_free_lottery_draws` | critical | Y | The lottery hashed bytes that include nonce and timestamp: one inference bought `2^22` tickets (ADR-0072); in the algo-4 era a seed without the timestamp allowed ~2×10^5 finalizer grinds per inference. | `mem:palw-adr-0072-the-ticket-is-the-execution`; `mem:palw-algo4-era-archive` | FIXED — `execution_commitment_v3` with a verifier-derived anchor (`consensus/core/src/palw_attempt_v2.rs:550-562`) |
| `unpinned_priced_field_free_draw` | critical | Y | A producer-chosen field inside the priced bytes that no rule pins is a nonce: sweeping `trace_retention_daa` over one execution gave 4,096 distinct tickets (9 admitted at `2^-9`). | `mem:palw-adr-0072-review-free-field-is-a-free-draw` | FIXED — ADR-0072 D8 pins (count, manifest, retention) by equality |
| `short_job_same_job_id` | high | Y | The seat's material check compared only `job_id == claim.anchor`; a producer ran a smaller job (skipped decode, shorter prompt) under the same id and seats holding material approved it. | `mem:an-id-check-is-not-a-check-of-what-the-id-names`; R152 SEAT-S1 | FIXED — whole-job comparison (ADR-0117; SEAT-S1 `d72c6e95`) |
| `borrowed_root_claim` | critical | Y | A claim re-uses another producer's honest roots: failures cannot be attributed (every DA accusation is answered by the lender's data), and the mint deduplicated one Final per root by lowest claim id, so a borrower took the lender's tickets. At 4 Sybils the EV went from −2,231 to +485 MSK per attempt. | R152 F1, M-1 | FIXED — `job_identity` (v22), contradictions 9–12, `PalwExecFinalV1` acceptance order (`ff2a070e`); J1 auto-filing PENDING (`rcore/p2-file`) |
| `accepted_but_unprosecutable_claim` | critical | Y | A claim the chain admits but no court can try: the court walker capped at `2^22` against a `2^26` ruleset (U-08); a class admitted under a declared worst case its legal jobs exceed by up to 18.4% (F4); openings larger than a carrier transaction; a close size between two units of one ceiling. | M0905 O-1; RW0919 F4; `mem:palw-audits-2026-08-26-to-30-archive` #13; `mem:one-ceiling-two-units` | FIXED — `palw_context_ladder` armed on t12 (`consensus/core/src/config/params.rs:15808`), deepest-legal-job gate, 400 KiB opening cap |
| `unattributable_2m_claims` | critical | Y | On the 2M held class a forged attention result cannot be attributed to a signer and full replay (~9.7 days) exceeds every deadline; with a single-draw lottery, opening the class would leave ≈38–46M MSK/month uncovered. | R152 (`mem:held-attention-attribution-8k-gate-2m-cap`) | PENDING — 2M closed at launch by `palw_class_verify_deadline` (`2ce3094a`) |
| `registration_signature_partial_preimage` | critical | Y | The class-registration signature covered 5 of 9 fields and nothing bound it to its carrier, so a registration could be re-bodied; it was also a bearer token valid forever. | M0828 M2-6, M2-18 | FIXED — whole object signed; network domain binds genesis |
| `fp_da_obligation_self_written` | medium | Y | A free-prompt claim's DA obligation is three numbers its own producer writes (one not carried at all), so the producer sets how long it must keep data. | M0906 M-3; M0905 unjudged | UNKNOWN |

### 3.3 pol — execution, adjudication arithmetic, work measurement

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `declared_canonical_job_weight_inflation` | critical | Y | Fork weight `claim.pwu = expected_attempts × pwu_per_inference` read the registrant's declared canonical job: the decode declaration gave 7.8× weight for identical arithmetic, and decode + `tile_len` reached 24,572× weight per executed MAC-eq; the same free split moved the slashable reservation without moving the weight. | RW0919 F1; RW0920 §1; `T:…/audit_repro_00_canonical_job_split_moves_the_slashable_.rs` | FIXED — `PwuClaimNotDerived`: pwu equals the derivation (ADR-0149, `f0896a34`), measured 1.000000× |
| `declared_class_target_free_weight` | critical | Y | Past the work target, pwu was priced from a registrant-declared, never re-priced class target: `initial_target = 1` gives `pwu = u64::MAX`, credited as immature then safe weight; honest model classes were disqualified (drill: 15 rejections on 8 nodes). | A0918 C-2 (:64-98); M0828 M2-12; `mem:palw-adr-0076-class-target-seed` | FIXED — `palw_effective_class_target_v1` (`2207f0df`); `8a2f7e96` |
| `attention_geometry_price_inflation` | critical | Y | A non-fused class's `attn_heads × attn_head_dim` priced attention nodes and was bounded by nothing (16,421×); after the fix the maximizing input moved to GDN geometry (820×) and then the conv kernel (7×). | AUD0923 C-4 (`T:…/audit_repro_02_nonfused_attention_geometry_is_an_unboun.rs`, `audit_ratio_ci_guard.rs`) | FIXED — `attn_heads × attn_head_dim ≤ query row`, `palw_gdn_geometry_fits_its_rows_v1`, conv kernel ≤ 64 (`b5c5f324`) |
| `fp_self_reported_work` | critical | Y | The free-prompt lane's `work_leaves` is the executor's own field: ×10 still passed, a padded prefix moved pwu 62×, and two forward passes bought the 63-quantum jackpot because the seat's sample denominator came from the accused job. | RW0919 F2; M0906 C-3 (:280-360) | FIXED — declaration is a comparand, never an input (`c37644e6`, `78dc747f`) |
| `uncertified_class_fake_weight` | critical | Y | A class whose fraud no court can convict (uncertified family, no responder) still bore weight: one fake block per epoch ≈ 8 epochs of honest safe weight; on t11 Relaunch 5, 97.8% of cadence went to classes the code itself said must not carry weight. | `mem:palw-uncertified-class-weight-hole`; `mem:palw-weight-bearing-requires-a-responder`; M0906 C-5 | FIXED — ADR-0069 D7 `palw_uncertified_weightless` (t12 inherits it from `palw_rc_base_params`, `params.rs:15377`, `:15699`; `T:…/palw_adr0069_d7_fold.rs`), registry lifecycle (Probation), answerability predicate |
| `pay_falls_with_model_width` | high | Y | Leaves count activations while cost counts weights, so pay per unit of compute fell like 1/model width (6.8× spread): the best model is paid least. | RW0919 F5 | FIXED — FP lane prices compute (`c37644e6`) |
| `sampled_verification_evasion` | critical | Y | A sampling verifier is evaded by minimizing the lie's footprint: `P_detect = 1-(1-f)^q` (ADR-0027); one-token lies were caught 6.51% per claim at 300 tokens; on t12 S3-sampled Valids counted toward the quorum, making k effectively 1. | `mem:palw-slash-adr-0027-no-bft`; `mem:palw-adr-0098-seat-coverage`; R152 F4 (critical: S3 made k = 1 on t12) | FIXED — funded full replay; S3 never counts toward the quorum; SEAT-R (`b589622f`) |
| `adjudicator_convicts_honest_execution` | high | Y | The court's reference arithmetic disagreed with the engine and convicted honest producers: negative int32 read as non-finite float; one dtype per node where layers mix Q4_K/Q6_K; RmsNorm `eps_q = 1` hard-coded; per-head softmax, RoPE offset and P·V layout; MatMulQuant opening one row; Qwen3.6 FP worker missing the attempt rule. | TM P0-8; A0817 §3.3; `mem:palw-rc-launch-audit-2026-08-21` (`5cf1a94c`); `mem:rcore-int2-integration-0924` (`a4682a8d`) | FIXED — lanes in `shape_profile_id`, per-layer dtypes, full-leaf sweep (914/914) |
| `arithmetic_substitution_attacks` | high | Y | Cheaper or different arithmetic presented as the class's: reduction re-association, FMA smuggling, transcendental substitution, flash-attention re-enable, non-finite smuggling, domain bridge, forged kernel id, layer-kind confusion, routing relabel, ready-set forgery, a challenger choosing an unrelated input leaf. | `consensus/core/src/palw_adversarial.rs:40-470` (13 `attack_*` tests); `mem:palw-algo4-era-archive` (`ea16274`) | FIXED — canonical reference arithmetic in the class identity |
| `tolerance_band_model_substitution` | high | Y | A verifier that accepts outputs within a tolerance (rank overlap, p95 activation diff) accepts a cheaper or different model inside the band. | `docs/ambient-pol-binary-audit-2026-08-15.md` | BY-DESIGN — exact-within-class; no tolerance comparison reaches a slash path (`mem:palw-mainnet-audit-adr0028`) |
| `output_text_only_binding` | high | Y | Binding only the output text: 1-bit seed flips produced identical text (attractors), so the tag does not identify the execution. | `docs/palw-algo4-forgery-audit-2026-08-16.md`; `mem:palw-algo4-era-archive` | BY-DESIGN — bind the whole execution commitment; text-only binding banned |
| `accuser_authored_binding_conviction` | critical | Y | The refutation was bound only to itself and the public `trace_root`, or its signature omitted the committed root: any bond copied the public root, attached an invalid shape profile with no openings and convicted an honest executor at the cost of one message. | TM C3, P0-8 (:257-302); `mem:palw-credit-path-criticals-2026-08-17` (`edac4b4`); A0817 §3.3 | FIXED — `check_execution_root_binding` against the claim's own `execution_root` |
| `court_never_convicts` | critical | Y | Structural acquittal: the close carried empty operand openings (read as "no fault"); the court bisected one capture with two readers (always agree); role-split capture then made every challenger's terminal close inadmissible and slashed the challenger; the close binding matched no real claim. | `mem:palw-audits-2026-08-26-to-30-archive` #5; M0828 M2-4; M0829 S-03; R0822 #7 | FIXED — recording oracle, capture by role (`a4f28393`), logits-root binding (`8be04677`) |
| `self_chosen_close_step_acquittal` | critical | Y | The close was not bound to the dispute: the executor recomputed a correct step of its own choosing and was acquitted. | R0822 #8 | FIXED — the close must be the step the ladder narrowed to (`8be04677`) |
| `held_attention_consistent_forger` | high | Y | On a held class the per-position anchor sits after the disputed position, so a consistent forger's downstream rows are in the anchor, the honest challenger cannot build the bottom, the forger is acquitted and the challenger charged. | R152 (A-held C2) | PENDING — `CourtAttnRootClaimedHeld` (object 57, slice sub-roots; `72436728`) and the node's N3/N4/auto-answer (`2dcb95b6`) are on `feat/t12-aheld-node`, not in the reference |
| `decoy_dissection_preemption` | high | Y | One court per claim (`ShardCourtClaimUnderSession`): a forger's Sybil opens a decoy dissection first and shields the lie; a decoy at the lie's own leaf held off every seat. | R152 (A-held C3/C4, F2) | PENDING — `72436728`, `9bf7accf` on `feat/t12-aheld-node` |
| `deliberate_court_loss_slashes_signers` | high | Y | A colluding producer loses (or defaults) a court on an honest execution so the claim voids `CourtFraud`, and the false-Valid rule then slashes every honest full-mask signer (~11.7k MSK each for ~5k cost); committing accumulators is insufficient because the child fold is additive (±δ split). | R152 (F3, A-held F3); `mem:held-attention-attribution-8k-gate-2m-cap` | FIXED for defaults (void reason 7 CourtDefault, `ea53d7e6`); dissection verdicts PENDING (void reason 8 for every dissection, `343123e6`, `3ee76a99`) |
| `forger_race_challenger_forfeit` | medium | Y | A consistent forger closes "innocent" at the bottom block, so the seat loses its dissection stake even though a later checkpoint accusation convicts the same anchor. | `mem:rcore-int2-integration-0924` | PENDING — `c68479db` restores the forfeit on conviction |
| `weight_history_rewritten_by_retarget` | high | Y | pwu derived from the class's *current* target (no per-block history) rewrote the safe weight of already-matured history at every retarget; the dispute ladder replayed in raw slice order. | A0817 §3.2 | FIXED — V2 claims record pwu at acceptance [unverified]; canonical carriage order |

### 3.4 panel — seats, receipts, licensing, DA court

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `executor_judges_own_claim` | high | Y | The executor sat on its own panel: exclusion used a different id namespace, one key with two bonds drew `[X, X]`, eligibility was hard-coded `bonded: true`, and a sibling bond self-attested. | TM P0-7; `mem:palw-credit-path-criticals-2026-08-17` (`e4936d6`, `a3bf0ed`) | FIXED — `derive_panel_v2` from one registry, operator dedup, anchor-DAA eligibility |
| `registrant_self_certifying_panel` | high | Y | Panels drew only from bonds that declared capability for the class — normally the registrant's own — so self-certification costs six operator keys; capability sets were self-declared and unbounded. | RW0919 F3; M0905 O-12; `mem:palw-adr-0071-hash-coupling-remainder` | FIXED — ADR-0147 outsider seat with veto (`75a39d58`); `palw_capability_bound` armed on t12 (`params.rs:15800`) |
| `seat_sybil_by_cheap_identities` | high | Y | Seats were bought with cheap identities: 0.004 MSK bonds, a freely chosen `operator_id` key, one ticket per operator; an undetected colluding segment pair was +EV at ~20 uniform Sybil operators (2.60M MSK). | M0830 addendum; M0905 O-10; A0918 B-07; M0828 M2-19; R152 (Q4) | FIXED — producer floor 13,000 / seat floor 130,000 MSK, operator possession (`46e84b4d`, `5a559459` B1), stake-weighted draw (`f1dfb33b`; `palw_panel_v2.rs:150-176`) |
| `panel_draw_seed_grind` | high | Y | The sortition seed was a hash the accused can re-roll: a block hash at one hash per try, a heartbeat seed (`2^24`-grindable), a free `BindTimeout` redraw, or the anchor producer choosing among its winning draws. | M0905 O-6; R152 (stake integration (a)); ADR-0130 SA-4; ADR-0073 SA-1 | FIXED — anchor only on attempt blocks, one-shot anchor (BindTimeout at that block); residual SA-4 BY-DESIGN. *Chapter verdict: OPEN (real) — the seed still hashes the anchor's block identity (04 §6.4, 05 §6.2 D1; §3.13.2)* |
| `dual_quorum_opposite_licences` | high | Y | One `quorum` licensed both `Valid → ReceiptLicensed` and `Unavailable → ProducerDefaulted`; at 4 seats / quorum 2 both reach quorum and check order decides. | TM C5 | FIXED — `2·quorum > seat_count` |
| `relay_loss_convicts_honest_producer` | high | Y | `Unavailable` was a guilty vote, so seats that merely failed to receive material (≈30% of pulls lost silently) voided and slashed honest producers — 35–47% of t11 claims; the rule was not carried into t12's regenesis. | M0830 second addendum; `mem:palw-bond-is-cheap-permanent-and-buys-quorum` | FIXED — ADR-0065 D4 `palw_unavailable_abstains`, armed on t12 (`params.rs:15927`, `05343cdf`) |
| `seat_silence_mispriced` | medium | Y | Silence is not checkable: unpunished no-shows let a bond take seats forever for free; charging unprovable silence slashed 2/5 seats on an honest network; with no reward, answering is all downside. | TM P0-7; M0828 M2-7; RA0829 R-7; `mem:palw-silence-is-not-checkable` | BY-DESIGN — silence abstains; accountability comes from signed verdicts (F2) and the RT#2 forfeit |
| `unavailable_majority_voids_free` | high | Y | With `Unavailable` abstaining, a seat majority could void every producer's claim for free, destroying escrowed reward. | M0905 O-9 | FIXED — DA court redesign (R-core+ M3), DA-confirmed withholding only |
| `falsevalid_unfileable` | critical | Y | The false-`Valid` conviction required `job_id == claim_id`, a hash fixed point no block-lane claim can meet, so a lying `Valid` signer could never be slashed. | R152 F2 | FIXED — `PanelFalseValidV2` (kind 3) under `palw_offence_attribution` (`params.rs:15914`) |
| `da_court_single_session_preemption` | critical | Y | The DA court had one session, one index and no pause credit, so a Sybil pre-empts the honest accusation and hold-to-Final forfeits nothing. | R152 F3 | FIXED — multi-index DA court (M3) |
| `honest_seats_slashed_by_unaligned_checks` | high | Y | False-Valid convictions slashed honest full seats whenever the seat's own checks were weaker than the conviction rule (seats did not compare `output_root`; the held branch checked only `job_id`). | R152 ship-gate (M2 review) | FIXED — SEAT-S1/S2/S4/SEAT-R ship in the same binary as the fence (`d72c6e95`, `a82e3dbb`, `ede34f83`, `b589622f`) |
| `executor_conviction_cascades_to_seats` | high | Y | An executor's conviction cascaded to honest seats; a colluding quorum that withheld data could not be convicted after `Final`; the burn was lost after claim retirement. | R152 X1, X2, X3 | FIXED — bind only the convicted claim; hold to Final on a Withheld filing; burn bound to the row |
| `free_da_accusation_griefing` | critical | Y | The node's DA responder answered no R-core session, so each accusation cost an honest floor producer 3,200.95 MSK while the accuser's exposure was refunded on default (~20 sessions per 13k bond); DA-7's reversal voided `CourtFraud`, slashing honest signers. | R152 (M3 review F1, F2) | FIXED — producer responder + `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` (P2-7 `6cbfefbf`); reason `ProducerWithholding` |
| `withholding_producer_uncharged` | high | Y | A producer whose claim never binds or never licenses was charged 0 while pinning others' seats: 1,280 MSK × 1,202 DAA against 0.000385 MSK — 3.3×10^6 capital-time amplification. | DOS0924 #9/#10 (`T:…/dos_repro_4_withholding_producer_charged_only_by_pol.rs`) | FIXED — withholding forfeits the escrow; ceiling on live state (`b38356fe`) |
| `seat_reward_exceeds_slash` | high | Y | A lying seat's reward far exceeded what it could lose (t11: reward 128 MSK vs slash 0.40 MSK; 129× to 110,700×); ADR-0151 found the colluding-quorum margin was 2 sompi. | `mem:t11-seat-concurrency-and-lambda-capital-0917`; `mem:palw-adr-0151-liveness-structural-collateral-fraud`; `T:…/dos_l3_quorum_and_rights.rs` | FIXED — λ = 2 floor (`params.rs:15866`), seat lock = residual + 10%·E/k (R-core+), 10% margin |
| `registrant_priced_seat_penalty` | high | Y | Seat penalties were `pwu × slash_value_per_pwu`, both registrant-chosen, so one registration drained others' bonds — and a drawn seat was slashed however it answered. | `mem:palw-audits-2026-08-26-to-30-archive` (#22/#24) | FIXED — seat exposure capped, `Incapable` answer |
| `producer_defaulted_unsigned_verdicts` | critical | Y | `ProducerDefaulted{receipts: []}` burned the producer's bond and every seat: the object carried unsigned seat verdicts and `validate_receipt_quorum_v2` had no caller. | `mem:palw-rc-launch-audit-2026-08-21` (`40002ddd`) | FIXED — signed `PalwSeatReceiptV2` |
| `forged_false_valid_equivocation` | high | Y | A `PanelFalseValid` with an `ExecutorEquivocation` contradiction was verified under a key carried in the evidence, so a stranger forged convictions that the reversal then trusted. | DOS0924 review (`T:…/review_economic_forged_false_valid.rs`, `dos_g2_conviction_takes_back.rs:585-640`) | FIXED — `e5f247fe` |
| `whole_collateral_slash_of_uncarried_signer` | high | Y | A `Valid` signer not carried by the licensing set held no lock, so a false-Valid conviction slashed its whole collateral (938,888 MSK for a genesis seat) instead of its lock. | DOS0924 review (`T:…/review_economic_whole_collateral_branch.rs`) | FIXED past the audit fence |
| `replay_budget_horizon_collapse` | high | Y | Panel room was measured over the minimum window of admitting classes with fallback horizon 1, and the numerator was global: one shorter-window class passing audit held every model class for 4–40 h. | `mem:replay-budget-horizon-collapse-holds-every-class` | FIXED — rate rule per class window (`e93be0f2`, `f8c91f19`) |
| `panel_room_squat_by_small_claims` | high | Y | A free-prompt claim counted as one whole claim on the room budget whatever its size, so the smallest commitments bought full attempt slots and closed a held class's attempt lane; one junk single-draw 2M claim put five seats on duty at `3 × reserved` and zeroed the 2M lane. | DOS0924 (`T:…/review_economic_fp_room_amplification.rs`, `dos_repro_3_one_junk_2m_claim_kills_the_2m_lane.rs`) | FIXED — whole-canon counting (`bc7f02dd`); option A capacity + #10; 2M closure PENDING |
| `panel_room_zero_readiness_halt` | high | Y | At the registry flag day the room check went live with zero readiness rows and a ready-seat predicate different from the draw's, so every non-base class returned `PanelRoomExhausted`/`NoCapablePanel`. | A0918 H-2, H-3 | FIXED — grace, one predicate (`2207f0df`) |
| `possession_proof_binds_index_only` | medium | Y | A readiness possession proof signs a leaf index, not the data, so "ready" means "can fetch 8 rows on demand". | A0918 H-6 | BY-DESIGN (documented V1 limitation) |
| `panel_bound_poisons_block` | critical | Y | Anyone can publish an unsigned `PanelBound` (the panel is a function of public state); where the class prices the seat lock above posted collateral the fold errs and the carrying block is disqualified. | AUD0923 C-3 (`T:…/audit_repro_04_unsigned_panelbound_disqualifies_the_car.rs`) | FIXED — lock-ineligible `PanelBound` inert (`b5c5f324`) |
| `reporter_reward_front_running` | medium | Y | A conviction's reporter reward can be stolen by copying the filing (or a re-encoding of it) into an earlier block. | R152 (user decision 5); `consensus/core/src/palw_state_v2.rs:55124` (`t39_a_copied_commitment_neither_blocks_nor_steals_the_reward`) | FIXED in consensus (commit–reveal); node filer PENDING (`rcore/p2-file`) |
| `panel_redraw_inconsistency` | critical | Y | After a redraw, the deadline index still used `accepted_daa` (every node panicked on a carriage mismatch) and the second panel could never bind; withdrawal and exposure horizons ignored the redraw (collateral out 901 DAA early). | M0830 §fixed in place | FIXED — horizons 7,200 / 7,500 |

### 3.5 bond — collateral, reserve, locks, exit

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `bond_split_amplification` | high | Y | Split one bond into many to multiply issuance, eligibility or votes. Real in the VLT overlay (the per-validator cap was applied per bond and summed: 2e9 vs 1e9); in PALW bounded by per-identity floors and operator dedup (Sybil is bounded, not prevented). | `mem:audit-2026-08-11-p0-status` (`dca5f94`, `splitting_a_bond_moves_no_weight`); TM (ADR-0042 Decision 7 note); `mem:palw-credit-path-criticals-2026-08-17` | FIXED — see §2. *Chapter verdict: OPEN (partial), §3.13.2* |
| `one_bond_backs_unbounded_immature_work` | critical | Y | One bond backed unbounded immature work: no exposure ceiling (P0-10), `slash_value_per_pwu = 0` made every reservation zero, the FP arm had no ceiling (8 jobs past collateral), the reservation ignored expected attempts, and the pure no-reserve design failed because a fake claim is indistinguishable from failure. | TM P0-10, H3; `mem:palw-adr-0072-the-ticket-is-the-execution` (`697f1d0c`); AUD0923 H-1; `mem:claim-collateral-design-bar` | FIXED — staged reserve (escrow + weight to licence, weight after), `ZeroSlashValue`, `palw_claim_attempts_v1` |
| `collateral_declared_not_locked` | critical | Y | `BondRegistered` declared its collateral and nothing locked a UTXO behind it ("I staked a million" staked a million); the collateral was spendable in the registering block; a merged transaction withdrew a DNS bond's 20M while the bond stayed Active; the wallet spent locked bonds. | TM P0-11; X0821 C-08; `mem:palw-audits-2026-08-26-to-30-archive` #6 (`b1a7644f`); M0828 M1-1, M1-3; `mem:wallet-spends-locked-producer-bonds` | FIXED — collateral is the carrier's output 0; spend gates at genesis |
| `lock_escape_before_conviction` | high | Y | Collateral escapes before a conviction lands: retire after `PanelBound` and before `Valid` so the lock lands on a retiring bond; burn lost after claim retirement; vested reward counted as collateral before a burn existed (≈2.88M MSK uncovered per 1,000 claims); G read from live facts. | DOS0924 (a); R152 X3, S-1…S-4 (H1, g_res) | FIXED — live lock read at retire, row-bound burn, `PALW_RCORE_VESTING_ROWS_LANDED_V1`, `g_res_sompi` (`3106e3d2`) |
| `exit_freeze_by_retirement` | high | Y | With 8 bonds, 5 seats and the executor excluded, the third retirement leaves too few eligible seats to bind: no Final, the second clock stops, every lock freezes forever (up to 4.13M MSK). | DOS0924 (c) (`T:…/dos_l4_two_clock.rs`) | FIXED — refuse retirements that break the draw + liveness escape |
| `lock_ledger_double_backing` | high | Y | The seat lock and the claim reservation counted the same collateral twice (150% backing); committed plus accuser exposure could exceed collateral (probe 120%). | DOS0924 finding 12; R152 (M1) | FIXED — one ledger (f12), `committed + accuser + new ≤ C` |
| `collateral_unit_mismatch` | high | Y | Collateral was posted in the class's declared leaves while the runtime reserved in floor-normalised MAC-eq (44.25× apart), so every t12 producer held at `produced = 0`; raw fork weight of one 2M Final is 3.36% of supply, 5,620× its reservation. | `mem:t12-collateral-was-posted-in-the-wrong-unit` (`2bd134ec`) | FIXED — `palw_exposure_unit_pwu_v1`; weight-unit coverage is the other half of ADR-0151 D1 |

### 3.6 econ — issuance, rewards, prices, rights

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `execution_quanta_overmint` | critical | Y | One Final minted execution permits in a unit 2,810× too large (1,585,742 quanta, a 2^16 mint cliff; 270,029 permits per held-2M Final), priced at zero because `extra_economic_rights` returned 0 — the colluding-quorum margin was 2 sompi. | AUD0923 C-2 (`T:…/audit_repro_01_exec_quanta_mint_has_no_byte_bound_while.rs`, `audit_a5_mint_cliff.rs`); `mem:palw-adr-0151-liveness-structural-collateral-fraud` (`f49c749d`, `19dfab48`) | FIXED — credit normalised (`b5c5f324`); `palw_economic_safety` (maturity, forfeit, fee-priced, 10% margin; `params.rs:15892`) |
| `conviction_leaves_final_rights` | high | Y | A conviction after Final took back nothing: safe weight, probe pass, usage, minted tickets and receipt-lane rights stayed. | DOS0924 #5/#7/#8 (`T:…/dos_g2_conviction_takes_back.rs`, `dos_repro_1_fp_final_receipt_rights_unpriced_unforfe.rs`) | FIXED — `8e28aa17` |
| `merged_work_payout_mismatch` | critical | Y | Merged work was paid differently from how the fold treated it: merged blues paid on entitlement alone (no lottery, budget or dedup); a voided merged claim forfeited nothing (escrow 0); the coinbase paid merged work the transition refused; receipt blocks' weight was minted but their coinbase denied. | R0822 #10; M0828 M2-3; M0829 S-02, S-07; `mem:palw-audit-2026-09-11-fence` (B-1 `bc5fca9a`); `T:…` fold merged re-check (`62815a48`) | FIXED — one predicate for fold and coinbase; merged escrow |
| `registration_moves_share_table` | high | Y | Registering a class moved everyone's price: one registration divided every incumbent's pay by 6.94; pending shares were written into the live table (1000‰ broken permanently); share grew on blocks whose claims voided (1‰ → 51‰ in 16 epochs); cadence share was purchasable at 1‰ per 40,000 sompi; a bought Final grandfathered Active. | RW0920 items 5–7; M0829 S-01; M0905 O-11; `mem:palw-adr-0107-share-growth-final` | FIXED — work target (ADR-0137), `e9636cb2`, `409e6545`, `palw_share_growth_final` (`params.rs:15857`) |
| `registration_spam_state_growth` | medium | Y | Weightless registrations were permanent, free and unbounded; share-0 classes were never reclaimed; public registrations replayed; each grows rooted state scanned per block. | M0828 M2-8; A0918 H-5; DOS0924 #12 (`T:…/dos_l5_2_registration_flood.rs`, `review12_1_economic.rs:284-307`) | FIXED — 4 registrations a block + 1 MSK burn, producer-floor bond, obligations pruned (`139c9215`, `d0542304`) |
| `fp_commitment_flood_state_growth` | medium | Y | A `FreePromptCommitted` puts ~612 rooted bytes into state for ~3,600 DAA at ~no collateral, and every chain block rehashes and clones the whole state. | DOS0924 (`T:…/dos_repro_2_free_prompt_flood_linear_per_block_cost.rs`) | UNKNOWN |
| `heavy_recompute_budget_griefing` | medium | Y | Failing objects that pay only fees consumed the per-block heavy recompute budget (Whole-13 contradictions). | R152 (M2 Phase 3 review) | FIXED — `493696de` |
| `court_default_cheaper_than_losing` | medium | Y | A producer silent in its court turn paid less than one that lost, so silence was the rational defence. | R152 (S-4 deviation 2) | FIXED — S2 action on default |
| `junk_claim_composite_stall` | high | Y | One attacker bond registers cheap classes and floods junk floor claims (a won floor draw costs hashes, not inference): the network stays alive while useful PALW execution stalls. | DOS0924 (`T:…/dos_l5_6_composite.rs`, `dos_l5_1_claim_flood.rs`) | FIXED — #9 + #10 |
| `expensive_tier_cheap_execution` | high | Y | Register an expensive-tier class and execute the cheap thing: value per real MAC-eq rises with the tier the registrant names. | AUD0923 lane B4 (`T:…/audit_b4_attacker_model.rs`, `audit_b4r_attacker_model.rs`) | UNKNOWN |
| `class_id_split_same_artifact` | medium | Y | A field that changes no executed arithmetic (`n_threads + 1`) mints a second class id at identical price and root, so one artifact registers more lanes. | AUD0923 I4 (`T:…/audit_global_invariants.rs:511`, `#[ignore = "OPEN FINDING …"]`) | OPEN |
| `three_spellings_of_work` | medium | Y | `verification_ccu` keeps the declared decode budget, so three spellings of one class's work disagree (floor 1.408×) and a declaration no execution performs moves the producer/panel split. | AUD0923 I2 (`T:…/audit_global_invariants.rs:324`) | OPEN |
| `algo4_credit_mint_holes` | critical | Y | The pre-V2 credit path: no signature verification, no `committed_root` dedup, credit outputs appended to the coinbase unbudgeted, `min_credit_interval` unenforced (leverage 11,655×: 116.5M MSK per bond), payee resolved by HashMap order (non-deterministic coinbase), pruned acceptance data read as empty (fail-open), dust refutations erasing credit. | `mem:palw-mainnet-audit-adr0028` (B1–B9); A0817 §3.4; `mem:palw-algo4-era-archive` (B15) | FIXED — path retired by ADR-0038/0042 (V2) |
| `unbounded_coinbase_fanout` | medium | Y | Deferred quality bonus and reserve drip emit one output per validator with no bound; the widened cap leaves one slot at a full mergeset; the drip cap applies to a 2-DAA epoch. | M0905 O-15; M0906 H-2, H-3, M-1 | UNKNOWN |
| `fp_prefix_kv_credit` | medium | Y | The KV-cache credit was directional, per bond and died with its claim, so the same conversation was priced differently by commit order. | RW0920 (KV row) | FIXED — `78dc747f`, `220cf2c6` |

### 3.7 fork — fork choice, weight, selection, private branches

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `private_daa_finality_acceleration` | high | Y | A private branch advances its own clock so its claims mature, its seats reach maturity and its frontier moves faster than on the honest branch. Historical form: free receipt/attempt rows ran DAA windows ~180× fast; seat maturity (ADR-0065 D1) is measured on the branch's anchor DAA; frontier provenance was shown unimplementable (ADR-0065 D2). | M0905 O-7; `mem:palw-bond-is-cheap-permanent-and-buys-quorum`; HB0924 | OPEN (partial) — see §2 |
| `heartbeat_padding_buys_frontier_key` | high | Y | Fork choice's first key is the frontier's *blue score*; each heartbeat is +1 blue score for `2^24` hashes and no bond, so K beats then one matured claim outrank 1,000 matured claims and flip both the deep-reorg and the IBD-commit gates. Adjudicated from critical to high because the sink search pops by GHOSTDAG blue work first and the comparator only vetoes downward. | HB0924 H1-1 (`T:…/hb_fork_weight.rs:118-176`), adjudication (`T:…/hb_adjudication.rs`) | OPEN — comparator unchanged (`consensus/core/src/palw_fork_choice.rs:72-78`) |
| `failed_lottery_blue_weight` | high | Y | A block whose lottery failed still earns blue work: an attempt header earns a constant `2^20` from the header alone and pays it to descendants when merged blue; historically the receipt lane earned full `calc_work(bits)`, and the attempt lane's blue work was a constant while its PoW stayed `bits`-priced. | `consensus/src/processes/ghostdag/protocol.rs:625-634`; M0906 C-1; M0905 O-13; `mem:palw-audits-2026-08-26-to-30-archive` (#1 free blue work, `1eee073c`) | OPEN (partial) — receipt 0, heartbeat ε FIXED; attempt constant remains |
| `sybil_bond_private_fork_frontier` | critical | Y | A 0.004 MSK bond, permanent and never matured, lets an attacker fork later, register Sybil bonds inside fork blocks, seat its own panels, self-license and extend `safe_frontier` — "a private fork collects no receipts" was false. | M0830 addendum (:192-265); `mem:palw-bond-is-cheap-permanent-and-buys-quorum` | FIXED — ADR-0065 D1 seat maturity, floors 13k/130k, stake draw; residual `private_branch_double_spend`. *Chapter note: the self-licensing route reopens through `panel_draw_seed_grind` (05 §7)* |
| `private_branch_double_spend` | high | Y | Pay on the public chain, build a private branch that settles a conflicting spend: needs the branch's anchors (winning draws, each an inference) *and* a licensing quorum of seats drawn on that branch; honest receipts on a private branch are not slashable on the public chain. | ADR-0129 §1 attack 2, Decision 5, SA-3 | BY-DESIGN — settlement depth counts settled anchors; large payments wait for depth |
| `unsigned_receipt_private_fork` | critical | Y | Receipt signatures were verified nowhere on the weight path, so forged receipts matured a fabricated fork to full safe weight; unsigned conviction carriage voided weight; an unsigned `BisectMove::Open` pinned any block Provisional. | A0817 §3.1 | FIXED — signed receipts/objects (V2) |
| `empty_fork_frontier_advances` | critical | Y | The frontier advanced only when the global unresolved set was empty, so a fork with no attempts advanced it for free: 60 empty blocks reached frontier 60 against an honest chain stuck at 1, and `decide_deep_reorg_v2` said Allow. | TM C2 | FIXED — frontier = deepest resolved-prefix Final |
| `prior_sink_weight_divergence` | critical | Y | Weight read the node's mutable bond view and capability at its previous sink, so two nodes with different applied sinks chose different tips for the same DAG (permanent partition). | TM P0-4; CA0819 divergence D | FIXED — candidate-scoped state (`palw_v2_weight_invariant_under_prior_sink`) |
| `node_local_input_in_fold` | critical | Y | A consensus fold read node-local data: the registry works map from a best-effort carriage store (A0918 C-1, chain split); the state root stamped from the store tip (t11 `df80394b`); live capability store / live adjudication reads (VLT era). | A0918 C-1; `mem:t11-df80394b-state-root-split-and-the-t12-fence-ledger` (`6adf8584`); `mem:audit-2026-08-11-p0-status` (P0-6/P0-7) | FIXED — fold is a function of chain and build (`2207f0df`) |
| `selection_sites_disagree` | high | Y | Header-selected tip, IBD, pruning and virtual used different authorities (blue work vs PALW); the IBD fork-choice gate was structurally dead; a pruned node silently disabled PALW rules. | TM P0-5; R0822 #9; `mem:palw-rc-launch-audit-2026-08-21` | FIXED — one comparator for every site; pruning-point state import. *Chapter verdict: OPEN (partial) — several sites still order by blue work (06 §7.2, T12-D8; §3.13.2)* |
| `fresh_tip_unresolved_fallback` | high | Y | A tip required a panel anchor after itself, so every fresh tip was "unresolved" and fork choice silently fell back to blue work. | TM P0-3; CA0819 | FIXED — immature claim is Provisional with β weight |
| `equivocation_keeps_fork_weight` | critical | Y | A valid equivocation certificate slashed the bond via the attestation path while the weight resolver verified it under the receipt context, so the fabricated block kept its weight. | TM P0-6; CA0819 attack C | FIXED — typed per-family signature contexts |
| `object_poisons_carrying_block` | high | Y | A stateful lie inside a relayed object disqualified the honest block that carried it (0x4b admission was stateless only). | `mem:palw-rc-launch-audit-2026-08-21` (`4724863a`); see `panel_bound_poisons_block` | FIXED — refused objects are dropped, the block stands |
| `pruning_witness_selection` | high | Y | One block poisoned a pruning point and the refusal landed after the UTXO set was cleared; the witness child was then selectable by one cheap block via the grindable hash tiebreak. | M0828 M1-2; RA0829 R-3 | FIXED — witness is the selected-chain child |
| `safe_frontier_scalar_not_rederived` | high | Y | `safe_frontier_blue_score` was a carriage scalar nothing re-derived; an eclipse attacker serving a staged headers-proof IBD pins a first-syncing node to its chain forever (the frontier is monotone). | M0905 O-2 | OPEN (partial) — bounded by the pruning point's blue score, not re-derived [unverified at a0af3c92] |
| `zero_weight_lane_hash_tiebreak` | medium | Y | A lane with zero blue work makes all its branches tie on work, handing fork choice to the block hash; the heartbeat-only regime needs ε, not 0. | `mem:palw-silence-is-not-checkable` | BY-DESIGN — heartbeat ε = 1 (`protocol.rs:611-624`). *Synthesis note: a regression risk in next, whose fork choice weighs heartbeats at zero (§3.13.2)* |
| `heartbeat_only_trap` | high | Y | Under `ghostdag_k = 1`, a slow bonded block (17-min draw) built on an old tip is merged red once a heartbeat is the selected parent, so the chain stays heartbeat-only and DNS finality stops. | `mem:t11-heartbeat-trap-stalls-dns` | FIXED — ADR-0105 `palw_heartbeat_transparent` (`params.rs:15855`) |
| `execution_block_burst_confirmations` | medium | Y | Present many fast blocks (round/execution/heartbeat) as confirmations. | ADR-0129 §1 attack 1, Decision 1 | BY-DESIGN — execution blocks move no frontier, weight, blue work or DAA |
| `header_level_and_parent_misread` | high | Y | Receipt headers' levels unclamped on the normal path; `direct_parents()[0]` used as the selected parent; `safe_frontier` naming a merged red. | `mem:palw-audits-2026-08-26-to-30-archive` (#19); M0828 M2-27 | FIXED |

### 3.8 finality — DNS/BFT overlay, reorg gates, VLT (retired)

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `dns_veto_expires_on_heartbeat_clock` | medium | Y | The DNS-BFT reorg veto — the one gate that refuses a reorg before the PALW comparator — expires after `dns_veto_ttl_daa_score` DAA, and heartbeats drive the DAA with zero bond and zero pwu. | HB0924 (`T:…/hb_fork_weight.rs:233-248`) | OPEN [unverified whether t12 relies on the veto] |
| `validator_count_by_bonds` | critical | Y | `min_active_validators` counted bonds, so one key with 12 bonds controlled DNS Active. | M0830 §fixed in place | FIXED |
| `inactivity_leak_window_mismatch` | high | Y | The leak's evidence window spanned ~150 s of blue score against a declared 7 days (2/12 branches got full credit); later it was walked in blue score while decided in DAA, so the leak never fired. | M0830; DAA0918 §8 | FIXED — `4006bfee`; leak re-fenced (ADR-0066) |
| `forged_slash_evidence_via_mergeset` | critical | Y | Forged slash evidence entered through the mergeset; a `ComputeChallenge` slashed whatever bond its payload named with no check. | `mem:audit-2026-08-11-p0-status` (`9685e0b`); `mem:compute-fraud-proofs-must-be-provable` (`e9a5754`) | FIXED — slash only on offences provable from the object |
| `unverified_overlay_snapshot_import` | critical | Y | A peer's overlay snapshot was written into live consensus during IBD without verification. | `mem:audit-2026-08-11-p0-status` (`1a7838d`) | FIXED |
| `dns_reorg_gate_wedge` | high | Y | The reorg gate's release TTL counted DAA on the node's own chain, which the wedge itself had stopped: a closed deadlock (~12 days instead of ~55 min). | `mem:t10-archive` | FIXED — TTL 6,000 → 2,000 + validator flag |
| `single_block_satisfies_work_depth` | medium | Y | On a carded mainnet one attempt block satisfies `required_work_depth`, collapsing the DNS overlay's PoW dimension. | M0905 (should-fix) | UNKNOWN |
| `vlt_committee_attacks` | high | Y | VLT-era: `ReplayProof` proved only self-consistency (later members copy it), committee sortition was grindable, a coarse determinism tag let honest verifiers refute honest executors. | `mem:audit-2026-08-11-p0-status` | FIXED — subsystem retired (ADR-0134) |

### 3.9 network — p2p, IBD, gossip, identity handshake

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `unauthenticated_material_gossip_amplification` | critical | N | PALW material and receipts were flood-relayed before any binding to a live claim: unbounded disk/RAM (~10^5 amplifier), a pool capped per claim only, 560 B/min switching off a node's serve budget, an 80-byte request buying a 16 MiB blocking read, a solicited exemption evicting the honest answer. | M0828 M2-1, M2-2; M0829 S-04, S-12…S-15; M0905 O-17; R0822 #11 | FIXED (node) |
| `uncached_state_materialization_per_request` | high | N | A 40-byte p2p request (pruning-point PALW state), an uncapped `classCarriages` list, or an unauthenticated wRPC call ran full uncached state materializations and pinned the single IBD latch. | M0906 H-1, M-5; M0905 O-16 | FIXED / partial [unverified] |
| `handshake_and_quarantine_abuse` | high | N | An unbounded peer blob expanded into a multi-GB string during the handshake; a sidecar-import refusal or a peer's normal "found: false" permanently quarantined a node. | M0829 S-09, S-10, S-11; `mem:palw-audit-2026-09-01-network-kill-panics` (F19) | FIXED |
| `gossip_prealloc_abort` | critical | N | Gossip allocated from a peer-supplied u64 before bounds checks (64 B × 2^40 → uncatchable abort); blobs relayed before decoding. | `mem:palw-audits-2026-08-26-to-30-archive` #7 | FIXED |
| `header_buys_inference_before_parent_check` | high | N | Algo-4 era: each peer header bought one full LLM inference before parent checks, serialised on a global mutex; pruning-proof headers computed PoW before the algo check (one-message panic) and skipped shape checks. | A0817 P0-1…P0-3 | FIXED — no model on full nodes (TM W1) |
| `pruning_proof_single_lottery_off` | high | Y | The pruning-proof validator checked attempt PoW with the single-lottery fence hard-coded off, so no new node could join past 6,001. | A0918 H-1 | FIXED — `2207f0df` |
| `mutated_witness_poisons_block_id` | medium | Y | If block identity were the attempt id, a third party flipping one signature bit produces the same id with an invalid witness and poisons the honest block in every known-invalid cache. | TM P0-1 (Decision 3c) | BY-DESIGN — identity hashes raw carrier bytes |
| `unknown_op_drops_connection` | low | N | An old node closes the whole WebSocket on an unknown wRPC op, killing every later read on that connection. | RW0920 §2; `mem:unknown-wrpc-op-drops-the-connection` | BY-DESIGN (clients probe on a disposable connection) |

### 3.10 node — honest-node robustness, panel service, resources

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `poison_block_panics_every_node` | critical | Y | Attacker bytes reached arithmetic that panics in virtual processing (release builds keep `overflow-checks`): unvalidated DecodeToken geometry, a negative exponent shift, unbounded GDN dimensions (`1<<24`), GDN modulus, `grouped_fast` checking channel 0 only, a two-input PQ dust transaction dividing by zero. The poison block is stored and relayed first, and a restart hits it again. | `mem:palw-audit-2026-09-01-network-kill-panics` (F25, F15, GDN); `mem:palw-audits-2026-08-26-to-30-archive` #4; M0905 #5; M0906 C-4 (:361-418) | FIXED — each site; fail-closed suite (TM) |
| `one_object_halts_the_chain` | critical | Y | A single object drives the state machine into a deterministic failure on every node: an abandoned FP claim's double deadline underflowing, a ~5 KB registration whose `n_ctx × layer_count` loop never ends, a court session outliving its claim, a pending-chunk index outside `0..count`. | `mem:palw-audits-2026-08-26-to-30-archive` #2; R0822 #2, #3; M0905 (should-fix `palw_state_v2.rs:8735`, later cleared by M0906) | FIXED |
| `panel_mempool_accept_treated_as_landed` | high | N | The panel recorded a submission at mempool acceptance and never cleared it: 431/579 claims a day never re-licensed (~1.8M MSK/day escrow burned on t11), and the same set kept material forever (0 → 11.2 GB in 12 h). | `mem:palw-audits-2026-08-26-to-30-archive` | FIXED |
| `licence_assembler_stall` | high | N | Assemblers kept only the first 2 `Valid` receipts in arrival order, so a late full seat made about half of floor claims never license. | `mem:floor-licence-stall-greedy-assembler`; `mem:t12-prelaunch-0924-panel-room-and-licence-stall` | FIXED — `a4dfe903`, `d94d3a1b` |
| `honest_node_resource_exhaustion` | high | N | Honest duties exhausted the host: dense materialization of a held FP claim (41.8 GiB), O(leaves) inventory roots (11.5 GiB), per-tick resolves, replay memory budget taken as the max over all artifacts, unbounded concurrent interval openings. | DOS0924 #4 (`T:…/dos_repro_0_held_class_dense_materialization_defeats.rs`); A0918 H-4; `mem:the-inventory-root-walk-is-o-leaves-in-memory`; `mem:item-6-acceptance-found-three-more-oom-paths`; `mem:a-request-scoped-limit-is-not-a-node-scoped-limit` | FIXED (node policy) |
| `forged_filing_poisons_node_cache` | high | N | A forged held root-claim filing (tag 57) poisoned the node's N2 cache (decoy-57); a replayed root-claim signature with a foreign binding could crowd out the filer. | `mem:rcore-int2-integration-0924`; `consensus/src/pipeline/virtual_processor/tests.rs:14563` | PENDING (decoy-57, `d2752b54`); crowd-out FIXED |
| `panel_arity_mismatch_defaults_honest` | high | N | The node's dense root claim derived its court arity without the held context (`NoAdmissibleArity`) while the processor derived 4, so an honest non-held fused producer never filed move 1 and was defaulted. | `mem:t11-panel-root-claim-arity-mismatch` | PENDING — `d2752b54` on `feat/t12-aheld-node` |
| `honest_node_misconfiguration_traps` | medium | N | Transient spawn errors (EAGAIN/ENOMEM) rejected honest blocks as failed PoW; anti-equivocation stores recorded in memory before the durable write; the bond-registration flag spent the bond; the fee scan selected locked collateral. | TM L1, L2; `mem:palw-audits-2026-08-26-to-30-archive` #29; M0828 M2-13; M0905 unjudged | FIXED |

### 3.11 evm — bridge, model market, positions

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `bridge_withdrawal_exceeds_backing` | high | N | The bridge is burn-and-mint with no L1-side ledger, so withdrawals were not capped by what was bridged in. | `mem:evm-bridge-ledger-fad522c6`; `T:consensus/core/tests/evm_bridge_ledger_is_t12_only.rs` | FIXED — `dee8b595` (t12 only) |
| `unbound_model_sink_output_burn` | low | Y | Consensus accepts a model-market sink output in any transaction without a bound market object: the MSK dies and the fold records nothing (not `burned_sompi`, no reserve, no refund). | `mem:unbound-model-sink-output-is-a-silent-burn` | OPEN — `consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs:102-110` |
| `model_market_payout_unwithheld` | high | Y | Every model-market payout was minted into the coinbase with nothing withheld; the market was an unbounded second writer into an 8-per-block payout queue; `model_positions` was an unbounded attacker-keyed table. | M0906 M-10, M-12, M-13 | UNKNOWN |
| `model_sell_bearer_signature` | medium | Y | A `ModelSell` signature covers no nonce, carrier or network domain, so it is a permanent bearer authorisation. | M0906 M-11 | UNKNOWN |
| `market_carrier_value_loss` | medium | Y | A refused carrier buy was not refunded, a carrier sell's net went to an unspendable payload, and a line's seed/buy did not wait for the class to be admitted. | `mem:t12-route-matrix-fixes-5a559459` (P-B1, P-B3, P-B4) | FIXED — `e758cc35`, `10e5b5c2`, `d6d92fb5` (t12 line only) |

### 3.12 other — identity, replay across networks, integrity

| name | sev | C | mechanism | sources | t12 status |
| --- | --- | --- | --- | --- | --- |
| `cross_network_signature_replay` | high | Y | A signature valid on one network or incarnation is accepted on another: the ML-DSA sighash commits to neither network nor genesis and premine outputs sat on a network-independent txid, so private-chain spends replayed onto t12; slash certificates bound to their own `network_id`; registration signatures and `ModelSell` were bearer tokens; a signer and verifier used different domains (R-8). | `mem:premine-spends-replay-across-chains-sharing-the-card`; A0817 §3.3; M0828 M2-18; RA0829 R-8 | OPEN (partial) — t12 premine/community txids and genesis timestamp separated (`a94e1fb0`); sighash domain is a mainnet decision |
| `ruleset_change_without_identity_move` | high | Y | A rule change that does not move the consensus identity forks the network silently: code-only rule changes, a `const bool` gating validity, a fence at an already-scheduled height, Some-only hashing without the never-collapse, `DnsParams` hashed whole, Borsh enum variants inserted mid-list, a cleanup that deleted history rules. | `mem:palw-audits-2026-08-26-to-30-archive` (08-27 learnings); `mem:palw-adr-0066-heartbeat-out-of-bits`; `mem:a-fence-at-a-scheduled-height-is-invisible-to-the-fork-id`; `mem:a-some-only-fence-needs-its-never-collapse`; `mem:t11-borsh-enum-renumbering-0910`; `mem:t11-codex-cleanup-snapshot-is-a-silent-fork`; RA0829 R-1, R-2 | FIXED (process) — identity/schedule split (M1-6), exhaustive `for_each_fence`, version bumps |
| `registered_graph_differs_from_executed` | high | Y | The chain's registered class identity is not what the engine runs: a stripper deleted `attn_o` in all 48 layers; the Qwen3.6 graph misdescribed its engine; an artifact digest was used as an inventory root (twice); two of t12's three genesis roots are hand-entered literals with no in-tree derivation. | M0828 M2-9; `mem:artifact-digest-and-inventory-root-are-one-type-and-two-paths`; AUD0923 I7 (`T:…/audit_global_invariants.rs:879`) | OPEN — I7 hand-entered roots; other instances FIXED (`7327c4e0`) |

### 3.13 Additions and code-level corrections from the chapters (2026-09-25)

The chapter authors (01–08) read t12 code for their subsystems, and the synthesis pass
([09-invariants.md](09-invariants.md), [10-attack-model.md](10-attack-model.md)) reconciled their
names with this catalog. This section records the result. It renames no existing entry: a name a
chapter coined for an existing hole is listed as an **alias**; a hole the catalog did not have is a
**new entry**; a status the chapters contradicted from code is a **correction**. The normative
verdicts are those of 10-attack-model.md.

#### 3.13.1 Aliases (chapter name → permanent name)

| chapter name | permanent name | chapter |
| --- | --- | --- |
| `busy_producer_clock_starvation` | `clock_slot_rule_freeze` | 01 |
| `heartbeat_bits_lockout` | `heartbeat_rows_price_bonded_lane_out` | 01 |
| `heartbeat_red_trap` | `heartbeat_only_trap` | 01 |
| `private_silence_eases_target` | `private_work_target_easing` (new, §3.13.3) | 04 |
| `pwu_declaration_inflation`, `fake_class_profile` | `declared_canonical_job_weight_inflation` | 04, 05 |
| `one_bond_unbounded_immature` | `one_bond_backs_unbounded_immature_work` | 04 |
| `panel_anchor_regrind` | `panel_draw_seed_grind` | 05 |
| `licence_stall_greedy_assembler` | `licence_assembler_stall` | 05 |
| `panel_root_claim_arity_mismatch` | `panel_arity_mismatch_defaults_honest` | 05 |
| `decoy_tag57_cache_poisoning` | `forged_filing_poisons_node_cache` | 05 |
| `decoy_court_session_monopoly` | `decoy_dissection_preemption` | 05 |
| `forgers_race` | `forger_race_challenger_forfeit` | 05 |
| `readiness_proof_outsourced` | `possession_proof_binds_index_only` | 05 |
| `default_as_signer_evidence` | `deliberate_court_loss_slashes_signers` (its default half) | 05 |
| `quorum_exclusivity` | `dual_quorum_opposite_licences` | 05 |
| `silence_is_not_checkable` | `seat_silence_mispriced` | 05 |
| `report_front_running` | `reporter_reward_front_running` | 05 |
| `frontier_blue_score_padding` | `heartbeat_padding_buys_frontier_key` | 06 |
| `finality_ttl_release` | `dns_veto_expires_on_heartbeat_clock` | 07 |
| `sybil_seat_capture` | `undetected_coverage_lie` (new, §3.13.3) | 02 |
| `withdraw_before_conviction` | `lock_escape_before_conviction` | 02 |
| `private_daa_vesting_release` | `second_clock_heartbeat_escape` (new, §3.13.3) | 02 |
| `withholding_amplification` | `withholding_producer_uncharged` | 02 |
| `wrong_unit_collateral` | `collateral_unit_mismatch` | 08 |
| `silent_sink_burn` | `unbound_model_sink_output_burn` | 08 |
| `unmeasured_class_admission` | `unattributable_2m_claims` | 08 |
| `held_attention_lie_8k` | `held_attention_lie_unattributable` (new, §3.13.3) | 08 |

#### 3.13.2 Code-level corrections (t12 at a0af3c92)

| name | source status above | code-level verdict | evidence |
| --- | --- | --- | --- |
| `panel_draw_seed_grind` | FIXED | **OPEN (real)**. The fix above constrains the anchor *choice*; the seed still hashes the anchor's block identity. | Seat, operator, stake and outsider tickets hash `anchor_block` (`consensus/core/src/palw_panel_v2.rs:1480-1510`, `:1664-1684`), a block-identity hash over the real nonce and timestamp (`consensus/core/src/hashing/header.rs:165-174`), while the execution anchor keys only the bucket `nonce >> 22` (`consensus/core/src/palw_attempt_v2.rs:213`, `:233-235`, `:528-538`). 04 §6.4, 05 §6.2 D1; re-checked in synthesis. |
| `bond_split_amplification` | FIXED | **OPEN (partial)**. Issuance, tickets and the exposure ceiling are split-neutral; panel seats, action tiers, strikes and per-bond allowances are not. | Successive sampling without replacement, one seat per operator, weight cap 1,000,000 MSK (`consensus/core/src/palw_panel_v2.rs:35`, `:137`, `:1729`); tiers `min(x‰·C₀, 3G)` (`consensus/core/src/palw_state_v2.rs:3294-3316`); strikes per bond (`:3337-3348`); `⌈c/2⌉` class share (`:12239`); 64 reporter commitments per bond (`:967`). 02 §6.2–6.3; seat, share and reporter lines re-checked. |
| `failed_lottery_blue_weight` | real at GHOSTDAG, [unverified] sink impact | **OPEN (real)**, including the sink. | The header-only `2^20` (`consensus/src/processes/ghostdag/protocol.rs:666-671`) orders the sink heap (`consensus/src/pipeline/virtual_processor/processor.rs:13636-13644`) and every selected parent (`consensus/src/processes/ghostdag/protocol.rs:216-220`); Layer 0 admits any attempt digest past the single lottery (`consensus/pow/src/lib.rs:591-596`). 03, 04, 06 T12-D5; re-checked. |
| `bondless_attempt_row_grind` | OPEN | **partial**: the `bits` payoff is closed on t12; the header grind stays open and pays through `failed_lottery_blue_weight`. | Attempt rows are unpriced once the single lottery is active (`consensus/src/processes/difficulty.rs:357`, `:575`), and with no priced row the retarget answers the maximum target (`:583-585`); t12 arms the lottery from genesis (`consensus/core/src/config/params.rs:15536-15538`, `:15938-15944`); the doc's "C-1 stays open" (`params.rs:2457-2465`) now describes the grind, not the `bits` payoff. Synthesis; re-checked. |
| `w_controller_counts_nonfinal_blocks` | UNKNOWN | **OPEN (partial)**, bounded ×4 per epoch and by forfeits. | `produced_blocks` is incremented at acceptance (`consensus/core/src/palw_state_v2.rs:28543-28558`), never decremented on void (pinned by the test at `:40913-40918`), and read by the `W` step (`:22748-22755`). 03, 04; re-checked. |
| `selection_sites_disagree` | FIXED | **OPEN (partial)**: the comparator decides deep reorgs and IBD commits only. | The sink heap, selected parents, headers-proof acceptance and bootstrap adoption order by blue work (`consensus/src/pipeline/virtual_processor/processor.rs:13636-13644`; `consensus/src/processes/ghostdag/protocol.rs:216-220`; `consensus/src/processes/pruning_proof/validate.rs:488-551`; `protocol/flows/src/flowcontext/bootstrap_recovery.rs:274-300`); `select_palw_tip_v2` has no production caller (`consensus/core/src/palw_fork_authority_v2.rs:43-45`). 06 §7.2, T12-D8; re-checked. |
| `heartbeat_clock_acceleration` | BY-DESIGN | **partial**: rate closed on the honest chain; an unbonded lane still paces every absence, release and refill rule, and a private branch pays ~2×2^24 hashes a tick. | 01 §6.3 F1–F5, §7. |
| `sybil_bond_private_fork_frontier` | FIXED | closed for the 0.004 MSK route; the underlying "a private fork licenses itself" reopens through `panel_draw_seed_grind`. | 05 §7 (`private_fake_root_burst`). |
| `zero_weight_lane_hash_tiebreak` | BY-DESIGN | by design on t12 (ε = 1); **regression risk in next**, whose fork choice gives heartbeats zero weight (06 FORK-R8/R9). | 10-attack-model.md §6. |
| `fresh_tip_unresolved_fallback` | FIXED | fixed on t12 by giving `Provisional` claims β live weight, which is itself `unverified_live_weight`; next removes that weight (06 FORK-R7) and must not fall back to blue work (FORK-R8). | 06 T12-D4. |

#### 3.13.3 New entries

| name | class | sev | C | mechanism | source | t12 status |
| --- | --- | --- | --- | --- | --- | --- |
| `heartbeat_future_stamp_step` | time | high | Y | Split from `heartbeat_clock_acceleration`: with 132 s of drift against a 120 s slot, a beat stamped ref+120 s is granted at once and a step stamped "now" becomes the next reference, so the clock ticks every few seconds. | 01 §7; ADR-0142 §9.2 | FIXED — H5 (`consensus/src/pipeline/header_processor/pre_pow_validation.rs:94-97`) |
| `heartbeat_sibling_step_delay` | time | medium | Y | Split from `heartbeat_clock_acceleration`: sibling steps tie on blue score and a hash tie-break lets a future-stamped step push the next slot back. | 01 §7; ADR-0142 §9.2 | FIXED — earliest-timestamp tie (`consensus/core/src/palw_clock_cursor_v1.rs:140-146`) |
| `heartbeat_width_burst` | time | medium | Y | Sibling beats, or an unpaced beat chain, each valid alone and merged in bulk, add blue score. | 01 §7 | FIXED — ≤ 4 per mergeset (`consensus/src/pipeline/header_processor/post_pow_validation.rs:153-195`) |
| `clock_reference_window_escape` | time | low | Y | 264 blue blocks at one DAA score push the reference out of the window; the next beat is granted unconditionally and the floors are void for that step. | 01 §6.3 F7 | OPEN — one free tick (`consensus/core/src/palw_clock_cursor_v1.rs:118-122`) |
| `second_clock_heartbeat_escape` | time | high | Y | The residual of `economic_deadline_on_heartbeat_clock`: after `2 × window_court` DAA without a licence the second clock switches off, and locks, withdrawals and seat maturity release on DAA alone, which heartbeats drive. Vesting is blocked during the halt itself and matures once a licence resumes past expiry + `2 × window_court`. | 01 §6.3 F3; 02; review | OPEN (by design in t12) — `consensus/core/src/palw_state_v2.rs:2224-2229`; `consensus/core/src/palw_panel_var_v1.rs:241-243`, `:246-259`; vesting `consensus/core/src/palw_vesting_v1.rs:494`, `:509` |
| `private_absence_conviction` | time | high | Y | On a private branch honest seats, responders and validators cannot act; local deadlines convict them for absence (receipt timeout, court no-show, DA default, inactivity leak, reclamation). | 01 §6.3 F4 | OPEN — effective only if the branch wins fork choice |
| `private_work_target_easing` | time | medium | Y | A branch that crosses DAA epochs with few model claims eases `W` by the ÷4 clamp to `W₀` and eases the pooled receipt target, so its tickets get cheaper. | 01 F5; 04 §7 | OPEN — bounded by `W₀` (`consensus/core/src/palw_state_v2.rs:22736-22775`) |
| `blue_depth_unit_mismatch` | time | medium | Y | Finality and pruning depths are blue-score counts sized from DAA windows; heartbeats and attempts run blue score 3–6× faster than DAA. | 01 F6; 07 T12-F1 | OPEN (partial) — `consensus/core/src/config/params.rs:2781`, `:2697-2718` |
| `open_claim_frontier_pin` | claim | medium | Y | Any open claim, fabricated or not, holds the safe frontier below it until it resolves. | 03 §7 | OPEN (partial) — `consensus/core/src/palw_state_v2.rs:20919-20944` |
| `optimistic_single_seat_licence` | claim | high | Y | One full-replay `Valid` (the optimistic single-replay door, ADR-0133's "S2") licenses a claim that only one party replayed. | 05 PANEL-R15 | FIXED — basis ≥ 2 required for Final: the Final gate redraws, then voids, a licence awaiting replay (`consensus/core/src/palw_state_v2.rs:23744-23771`) |
| `held_attention_lie_unattributable` | pol | high | Y | An arithmetic lie in an `AttnFused` leaf of a held-context class (8k, 2M) has no conviction route; a V1 quorum licenses it. | 05 D5; 08 §6.3 | OPEN — PENDING A-held (`feat/t12-aheld-node`) |
| `undetected_coverage_lie` | panel | high | Y | Hold both attesters of a segment (P2) or a V1 quorum (P3) and license a lie no honest party can refute; the residual of `seat_sybil_by_cheap_identities`. | 02, 08 §4.5 | OPEN (partial) — thresholds 17.29M / 6.63M MSK (ADR-0152 §4.3) |
| `colluding_quorum` | panel | high | Y | Three colluding whole-job `Valid`s license false work; detectable by one honest replay filed in the window, priced by locks. | 05 §7 | OPEN (partial) |
| `false_valid_signer` | panel | high | Y | A seat signs `Valid` on work a contradiction later refutes; convictable except over-size proofs, held attention and out-of-segment faults. | 05 §7 | OPEN (partial); node filer PENDING (`rcore/p2-file`) |
| `silent_quorum_griefing` | panel | medium | Y | Seats stay silent; the first panel redraws, the second charges the honest producer while silent seats pay nothing. | 05 §6.5 | OPEN — accepted in ADR-0152 §4.2 row 12 |
| `admission_jury_seed_grind` | panel | high | Y | The admission jury seed hashes the seed anchor's block, so its producer re-rolls the jury. | 05 D3 | OPEN — PENDING seed v2 (`feat/t12-activation-pool`) |
| `admission_jury_sybil_capture` | panel | medium | Y | The jury is one ticket per operator at the registry floor; cheap Sybil operators win a `Candidate` majority. | 05 §7 | OPEN (partial) |
| `anchor_bind_censorship` | panel | medium | Y | A panel binds only in its anchor block; the anchor producer omits `PanelBound` and voids every claim anchored there, free. | 05 §7; review | FIXED at the reference — the chain derives the bindings a block owes; nothing is published to withhold (`consensus/src/pipeline/virtual_processor/processor.rs:11814-11828`, `:12290`, `:2053`; `consensus/core/src/palw_state_v2.rs:23733-23735`). Residual: the anchor hash grind, which is `panel_draw_seed_grind`. *(The chapters' first verdict, "real, medium confidence", misread t12.)* |
| `private_readiness_lapse_panel_capture` | panel | medium | Y | Readiness rows last 8 DAA and gate the draw past the 2026-09-23 audit fence; on a private heartbeat branch honest rows lapse while the forker refreshes its own. | 01 C52; review | UNKNOWN — lapse and gate read (`consensus/core/src/palw_model_registry_v1.rs:779-799`, `:859`); SW-10's eligible base not read |
| `private_self_licensing_branch` | fork | high | Y | On a branch it produces the adversary admits fake roots at the bucket rate and licenses them with its own seats (`P_cap(s)`, plus any ring shaping), so its verified weight, safe anchors and chain anchor grow independent of compute; it can bunch licences to lift its `SafeDaa`. | 10 §2.1 C11; review | OPEN — on t12 the forker re-rolls every panel on its branch (`panel_draw_seed_grind`), so `P_cap` approaches the chance its stake can fill a licensing set |
| `post_anchor_grinding` | panel | medium | Y | Register or deposit after seeing a seed to enter its panel. | 02 BOND-R4 | FIXED — ADR-0147 population cut (`consensus/core/src/palw_panel_v2.rs:272-292`) |
| `action_tier_dilution` | bond | medium | Y | Produce or sign from floor-sized accounts so permille-of-C₀ tiers stay small. | 02, 08 | OPEN — `consensus/core/src/palw_state_v2.rs:3294-3316` |
| `strike_evasion_by_split` | bond | low | Y | Rotate withholding across bonds so none reaches its third strike. | 02 | OPEN — `consensus/core/src/palw_state_v2.rs:3337-3348` |
| `vesting_escape` | bond | high | Y | Transfer, spend or pledge an unmatured reward. | 02 | FIXED — rows are PALW state, burnable (`consensus/core/src/palw_state_v2.rs:19948-19972`) |
| `weight_unit_gap` | econ | high | Y | Split from `collateral_unit_mismatch`: a verified fraudulent claim inserts fork weight worth far more than its reservation (5,620× on 2M). | 08 ECON-R12 | OPEN — `consensus/core/src/config/premine.rs:120-128` |
| `early_extraction` | econ | medium | Y | Realise the buyback, execution rights or fees of a fraudulent Final before a conviction can land. | 08 §6.4 | OPEN — `consensus/core/src/palw_state_v2.rs:19243`; `consensus/core/src/palw_economic_safety_v1.rs:95-113` |
| `self_report_capture` | econ | medium | Y | An offender files its own conviction to recover the reporter reward. | 08 ECON-R10 | FIXED — reward ≤ 10% of collected (`consensus/core/src/palw_state_v2.rs:970`) |
| `unverified_live_weight` | fork | high | Y | A `Provisional` claim no panel has seen adds β·pwu to the comparator's live key. | 06 T12-D4 | OPEN — `consensus/core/src/palw_state_v2.rs:28436-28443` |
| `path_dependent_sink_split` | fork | high | Y | The heap offers by blue work and the comparator vetoes only reorgs relative to the node's previous sink, so nodes with one DAG settle on different sinks. | 06 T12-D1 | OPEN — `consensus/src/pipeline/virtual_processor/processor.rs:13219` |
| `dns_gate_node_local_abstain` | fork | medium | Y | The DNS-BFT gate, asked before the comparator, abstains on a node-local evaluation failure and reads the incumbent's DAA. | 06 T12-D6 | OPEN (partial) |
| `ibd_asymmetric_weighing` | fork | medium | Y | IBD weighs the incumbent at its sink and the staged chain at the imported pruning point, after a headers proof accepted by blue work. | 06 T12-D7 | OPEN (partial) |
| `unweighable_fail_open` | fork | high | Y | A candidate whose PALW state cannot be weighed falls through to an allow. | 06 | FIXED — fail-closed (`consensus/src/pipeline/virtual_processor/processor.rs:13234-13248`) |
| `settled_claim_reverted_with_branch` | finality | medium | Y | A `Final` claim, and escrow released at Final, is final only on its branch and reverts with it. | 07 T12-F5 | OPEN (a property; harmful to readers that treat `Final` as irreversible) |
| `pruning_point_disagreement` | finality | medium | Y | The header-committed pruning point (blue score) and the node's local pruning (PALW ceiling) can differ. | 07 T12-F2 | OPEN (partial) |
| `pruning_deletes_evidence` | finality | high | Y | Pruning deletes history an unresolved claim or open court still needs. | 07 | FIXED for the local store (`consensus/src/pipeline/pruning_processor/processor.rs:223-228`) |
| `long_range_rewrite` | finality | high | Y | Bonds that have withdrawn sign an alternative past at no cost; a node without a recent trust root cannot tell it apart by weight. | 07 §3.5 | OPEN (partial) — trusted checkpoint exists (`consensus/core/src/config/trusted_checkpoint.rs:1-37`) |
| `receipt_pool_flush` | node | medium | N | Junk receipts evict genuine ones from a size-checked pool. | 05 §6.9 | FIXED (node) — `kaspad/src/palw_receipt_pool.rs:1-30` |

Design-level hazards of misaka-next itself (no t12 status; each is a regression test for the new
design, detailed in 10-attack-model.md §6): `pairwise_context_cycle`, `junk_candidate_context_drag`,
`stalled_leader_context_drag` (06 §3.2), `safe_mark_window_collapse` (09 INV-TIME-08),
`seed_ring_bootstrap_deadlock` (04 POL-R9, 09 INV-PANEL-12), `licence_halt_stake_freeze` (02 Q2-4,
07 Q5), `dispute_hold_griefing` (06 §3.3, Q3; added in review).

## 4. Coverage notes

- **Swept:** every memory file ranked by attack vocabulary and every audit index in the memory
  directory; `TM`, `CA0819`, `A0817`, `X0821`, `R0822`, `M0828`, `M0829`, `RA0829`, `M0830`, `M0905`,
  `M0906`, `A0918`, `DAA0918`, `RW0919`, `RW0920`, `docs/audit/2026-09-19-llm-mining/`; the
  Security-amendment sections of ADR-0071/0072/0073/0075/0125/0129/0130 in `docs/adr/`; the
  reference's fix commits (`git log a0af3c92`); the four pending branches' logs; the attack-named tests
  in `consensus/` (`palw_adversarial.rs`, `audit_*`, `dos_*`, `review*`, `hb_*`).
- **Not read in full:** the 60 dormant "high" items of A0817 beyond its root-cause summary, M0906's
  low findings, the external audit X0821's completeness items (C-01…C-07 are missing features rather
  than attacks), and ADR security amendments about host sandboxing (ADR-0079, 0078), which belong to a
  later milestone.
- **Chapter hand-off:** the `UNKNOWN` rows and every row marked [unverified] need a code-level verdict
  from the chapter that owns the rule.

## 5. Consensus-level regression list

misaka-next's adversarial simulator must reproduce each of these and the consensus model must refuse
it (or, for `BY-DESIGN` rows, hold it inside the stated bound). Where a seed invariant is the property
under attack, its ID is given.

**Clocks (INV-CLAIM-01, INV-CLAIM-02, INV-FORK-01)** — `heartbeat_clock_acceleration`,
`unpriced_lane_advances_daa`, `heartbeat_rows_price_bonded_lane_out`, `bondless_attempt_row_grind`,
`clock_slot_rule_freeze`, `clock_reference_node_local`, `economic_deadline_on_heartbeat_clock`,
`quantum_maturity_reads_wall_clock`, `retarget_ratchet_to_zero`, `idle_class_target_relaxation`,
`epoch_boundary_budget_mismatch`, `span_unit_mismatch_short_windows`, `w_controller_counts_nonfinal_blocks`.

**Claims and PoL (INV-POL-01)** — `private_fake_root_burst`, `one_execution_many_claims`,
`sibling_identity_pow_reuse`, `foreign_bond_signature_theft`, `nonce_free_lottery_draws`,
`unpinned_priced_field_free_draw`, `short_job_same_job_id`, `borrowed_root_claim`,
`accepted_but_unprosecutable_claim`, `unattributable_2m_claims`, `registration_signature_partial_preimage`,
`fp_da_obligation_self_written`, `declared_canonical_job_weight_inflation`,
`declared_class_target_free_weight`, `attention_geometry_price_inflation`, `fp_self_reported_work`,
`uncertified_class_fake_weight`, `pay_falls_with_model_width`, `sampled_verification_evasion`,
`adjudicator_convicts_honest_execution`, `arithmetic_substitution_attacks`,
`tolerance_band_model_substitution`, `output_text_only_binding`, `accuser_authored_binding_conviction`,
`court_never_convicts`, `self_chosen_close_step_acquittal`, `held_attention_consistent_forger`,
`decoy_dissection_preemption`, `deliberate_court_loss_slashes_signers`, `forger_race_challenger_forfeit`,
`weight_history_rewritten_by_retarget`.

**Panel** — `executor_judges_own_claim`, `registrant_self_certifying_panel`,
`seat_sybil_by_cheap_identities` (INV-BOND-01), `panel_draw_seed_grind`, `dual_quorum_opposite_licences`,
`relay_loss_convicts_honest_producer`, `seat_silence_mispriced`, `unavailable_majority_voids_free`,
`falsevalid_unfileable`, `da_court_single_session_preemption`,
`honest_seats_slashed_by_unaligned_checks`, `executor_conviction_cascades_to_seats`,
`free_da_accusation_griefing`, `withholding_producer_uncharged` (INV-ECON-01),
`seat_reward_exceeds_slash` (INV-ECON-01), `registrant_priced_seat_penalty`,
`producer_defaulted_unsigned_verdicts`, `forged_false_valid_equivocation`,
`whole_collateral_slash_of_uncarried_signer`, `replay_budget_horizon_collapse`,
`panel_room_squat_by_small_claims`, `panel_room_zero_readiness_halt`, `possession_proof_binds_index_only`,
`panel_bound_poisons_block`, `reporter_reward_front_running`, `panel_redraw_inconsistency`.

**Bonds and economics (INV-BOND-01, INV-ECON-01)** — `bond_split_amplification`,
`one_bond_backs_unbounded_immature_work`, `collateral_declared_not_locked`,
`lock_escape_before_conviction`, `exit_freeze_by_retirement`, `lock_ledger_double_backing`,
`collateral_unit_mismatch`, `execution_quanta_overmint`, `conviction_leaves_final_rights`,
`merged_work_payout_mismatch`, `registration_moves_share_table`, `registration_spam_state_growth`,
`fp_commitment_flood_state_growth`, `heavy_recompute_budget_griefing`, `court_default_cheaper_than_losing`,
`junk_claim_composite_stall`, `expensive_tier_cheap_execution`, `class_id_split_same_artifact`,
`three_spellings_of_work`, `algo4_credit_mint_holes`, `unbounded_coinbase_fanout`, `fp_prefix_kv_credit`.

**Fork choice and finality (INV-FORK-01, INV-POL-01)** — `private_daa_finality_acceleration`,
`heartbeat_padding_buys_frontier_key`, `failed_lottery_blue_weight`, `sybil_bond_private_fork_frontier`,
`private_branch_double_spend`, `unsigned_receipt_private_fork`, `empty_fork_frontier_advances`,
`prior_sink_weight_divergence`, `node_local_input_in_fold`, `selection_sites_disagree`,
`fresh_tip_unresolved_fallback`, `equivocation_keeps_fork_weight`, `object_poisons_carrying_block`,
`pruning_witness_selection`, `safe_frontier_scalar_not_rederived`, `zero_weight_lane_hash_tiebreak`,
`heartbeat_only_trap`, `execution_block_burst_confirmations`, `header_level_and_parent_misread`,
`dns_veto_expires_on_heartbeat_clock`, `validator_count_by_bonds`, `inactivity_leak_window_mismatch`,
`forged_slash_evidence_via_mergeset`, `unverified_overlay_snapshot_import`, `dns_reorg_gate_wedge`,
`single_block_satisfies_work_depth`, `pruning_proof_single_lottery_off`, `mutated_witness_poisons_block_id`.

**Consensus hygiene** — `poison_block_panics_every_node`, `one_object_halts_the_chain`,
`cross_network_signature_replay`, `ruleset_change_without_identity_move`,
`registered_graph_differs_from_executed`, `unbound_model_sink_output_burn`,
`model_market_payout_unwithheld`, `model_sell_bearer_signature`.

**Added by §3.13 (2026-09-25)** — clocks: `heartbeat_future_stamp_step`,
`heartbeat_sibling_step_delay`, `heartbeat_width_burst`, `clock_reference_window_escape`,
`second_clock_heartbeat_escape`, `private_absence_conviction`, `private_work_target_easing`,
`blue_depth_unit_mismatch`; claims and PoL: `open_claim_frontier_pin`,
`optimistic_single_seat_licence`, `held_attention_lie_unattributable`; panel: `undetected_coverage_lie`,
`colluding_quorum`, `false_valid_signer`, `silent_quorum_griefing`, `admission_jury_seed_grind`,
`admission_jury_sybil_capture`, `anchor_bind_censorship`, `post_anchor_grinding`,
`private_readiness_lapse_panel_capture`; bonds and economics:
`action_tier_dilution`, `strike_evasion_by_split`, `vesting_escape`, `weight_unit_gap`,
`early_extraction`, `self_report_capture`; fork choice and finality: `unverified_live_weight`,
`path_dependent_sink_split`, `dns_gate_node_local_abstain`, `ibd_asymmetric_weighing`,
`unweighable_fail_open`, `settled_claim_reverted_with_branch`, `pruning_point_disagreement`,
`pruning_deletes_evidence`, `long_range_rewrite`, `private_self_licensing_branch`; design-level:
`pairwise_context_cycle`, `junk_candidate_context_drag`, `stalled_leader_context_drag`,
`safe_mark_window_collapse`, `seed_ring_bootstrap_deadlock`, `licence_halt_stake_freeze`,
`dispute_hold_griefing`. `receipt_pool_flush` joins the node list
below.

Out of the consensus milestone (node, network, EVM; kept for the later milestones):
`unauthenticated_material_gossip_amplification`, `uncached_state_materialization_per_request`,
`handshake_and_quarantine_abuse`, `gossip_prealloc_abort`, `header_buys_inference_before_parent_check`,
`unknown_op_drops_connection`, `panel_mempool_accept_treated_as_landed`, `licence_assembler_stall`,
`honest_node_resource_exhaustion`, `forged_filing_poisons_node_cache`,
`panel_arity_mismatch_defaults_honest`, `honest_node_misconfiguration_traps`,
`bridge_withdrawal_exceeds_backing`, `market_carrier_value_loss`, `receipt_pool_flush`.

Retired subsystem, kept for the record only (the VLT overlay was removed by ADR-0134; next has no
committee): `vlt_committee_attacks`.
