# ADR-0162 … ADR-0178 — implementation status (2026-10-15)

Branch `integ/adr-0162-0178` (from `integ/rfc02-rfc06-12-g14`). **Nothing in this round was built, checked, tested or run** (user
rule: code first, a separate validation agent compiles and tests); no line below is a PASS. Every new consensus behaviour is
**dormant** behind a fence that is `None` on every preset (A-2 uniform: below its fence new kinds ride unjudged and the fold does
nothing; Some-only in the fingerprints). Live testnet-12 behaviour, presets and release heights are unchanged.

**Not covered — no source exists:** ADR-0160, ADR-0161 and ADR-0164 have no file in `docs/adr/` (`docs/adr/README.md:36`: such
documents are not reconstructed; ADR-0162 §12: 0160/0161 "held elsewhere"). The code's "ADR-0164" comments (×1000 package,
`params.rs:10999–11025`, `kaspad/src/args.rs:281`) refer to the package documented as **ADR-0167**.

Status words: **live** (in force on testnet-12 — several older fences are armed from DAA 5,300 by `palw_t12_arm_int11_flag_day_v1`,
`params.rs:11697`, list `PALW_T12_INT11_FENCES_V1` `params.rs:11343`, although some ADR texts say "in no t12 list");
**dormant** (implemented behind a `None` fence); **dormant — this round** (implemented here); **superseded / withdrawn**;
**docs-only** (no code by design); **evidence-only** (needs measurement / review / a real network); **user decision**; **missing**.

## Summary

| ADR | Title (short) | Status after this round |
|---|---|---|
| 0162 | The pair opens on a virtual reserve | dormant (`palw_model_virtual_v1`); owner leg 5 % live past `palw_model_leg_v2`; no code gap |
| 0163 | An adapter class is its parent plus an adapter | live on t12 from DAA 5,300; 2026-10-07 public-prosecution gate docs-only |
| 0165 | The floor is a reserve; work carries the clock | live on t12 from 5,300 (A″, B); FALLBACK-V1 withdrawn; §10.2–10.4 research |
| 0166 | Verifier unavailability is not producer fraud | live (C, D, E); 2026-10-12 "no early reuse after PanelUnavailable" **dormant — this round** (budget hold + formula slot); §6 standby seats open (design) |
| 0167 | ×1000 capacity package (code: "ADR-0164") | live on t12 (F-EM, F-M1, F-K, ρ steps); 2026-10-10 per-bond preservation = ADR-0176 (dormant); M-0a/M-0b not built (performance) |
| 0168 | An execution block is a third class | superseded by RFC-8 v2 (dormant `palw_exec_payload_v2`); D8 unverified |
| 0169 | Work slice as a normal consensus block | **superseded direction** (algo-11 deleted); replacement = RFC-8 v2 work-slice ledger (dormant) |
| 0170 | The seed anchor is a window | live on t12 from 5,300; complete |
| 0171 | Probabilistic constraint checks, court on dispute | partial, dormant (`palw_probabilistic_constraints_v1`, refused); §5 reviews evidence-only |
| 0172 | Versioned kernels, not a universal VM | implemented by absence + partial dormant kernel route; docs |
| 0173 (seat root) | A seat holds a root; possession gates activation | dormant (`palw_audit_1004_v1`); activation skeleton unwired; §7 user decisions |
| 0173 (purpose) | Public-verifier dispute completeness | partial dormant (G14 lanes); D9–D11 revised by 0177; D12 = ADR-0176 (dormant) |
| 0174 | Token name Misaka, ticker BILI | live (wallet/CLI/validator); **web display migrated this round**; naming-only |
| 0175 | Registered models are permanently immutable | dormant (`palw_model_immutable_v1`), refusal code complete |
| **0176** | **Bond bounds production, rewards and Final weight** | **dormant — this round**: mode-aware 1-day / 7-day hold, per-day equalized caps, burst + concurrent-liability caps, bond formula v2 (`palw_bond_formula_v2`), rule E hook wired, harness; values = user decision / measurement |
| 0177 | Model-bond allocation without availability consensus | **dormant — this round**: α = 1.5, 30-day miner + verifier average, P_m block opportunity; R-clip single application pending merge; D6/D7 evidence-only |
| 0178 | Fork choice heals a partition on bonded participation | dormant (`palw_fork_choice_rule_e_v1`, refused); §10 budget interface **wired this round**; §11 blockers user decision / evidence |

## ADR-0176 (priority) — decision items

| Item | Code | Status |
|---|---|---|
| D1 four caps per bond and period (`Q_max(C,W,ρ)`, `B_max`, `R_max`, `F_max`), ρ moves Q only | `consensus/core/src/palw_bond_budget_v1.rs` `palw_bond_budget_caps_v1`; V3 per-day form `palw_bond_budget_v1/hold.rs:221` (`palw_bond_hold_period_caps_v1`) | dormant (V3 this round) |
| D1 separate open-claim, unsettled-weight, max-liability caps | `palw_bond_budget_admit_v1` (open claims, `Σ K ≤ C`, unsettled F); `hold.rs:340` `hold_admit_v1` (concurrent liability `φ·C`, kept liability view) | dormant |
| D2 claim capacity × m splits one right (A → A/m), integer ledger, no rounding gain | `palw_bond_budget_claim_ceilings_v1`, rider attribution `palw_rider_block_attribution_v1`; formula rights `palw_bond_formula_v2.rs:370` | dormant |
| D3 reserve at acceptance, re-check at every writer, no un-reserved Final weight | engine `reserve*` / `consume`; fold hooks (Attempt, rider, FP, receipt spend, kernel, native funded v13/v14/v28/text29, Round rights, income rows); **rule E reader wired** `palw_fork_choice_rule_e_v1.rs:269/297` | dormant (H-4 this round) |
| D4 `reuse_not_before = d + W`, nothing earlier; collateral apart; history across split / exit / re-registration / key change / slash | engine releases only at `d + W`; withdrawal hold `window_holds` / `withdrawal_holds`; per-mode hold this round (`hold.rs`); licence no longer frees slot / escrow past `palw_bond_formula_v2` (`palw_issuance_slots_v1.rs`, `palw_state_v2.rs` flip) | dormant |
| D4 mode switching never re-acquires (ユーザー方針) | long view (all modes, 7 d) + day view (all claims, checked for Panel=1) | dormant — this round |
| D5 small bonds, rights and reservation split, no fictitious collateral | `palw_bond_formula_v2.rs` (decomposed reservation, measured-capacity gate) | dormant — this round; values user decision |
| D6 economic evaluation `p·(R+L) > C_saved` | — | evidence-only |
| §4 migration: old claims under old rules, no full-window re-acquisition at upgrade | Legacy seeding (`create_bond_budget_v1`); formula keyed on acceptance | dormant |
| 2026-10-11: fee-only Round in a separate ExecutionCap; market fee outside R in v1 | `validate_palw_bond_budget_v1` requires `ExecutionCap`; `MarketFee` origin | dormant |
| 2026-10-12: Bond/time ledger, three-layer budget | engine + allocation (network = emission budget, model = allocation, bond = caps) | dormant |
| **ユーザー方針 2026-10-15** (hold 1 d / 7 d, per-day equalization, burst, collateral ≠ capacity, heartbeat rules, measurement) | `hold.rs`; `palw_bond_cap_sweep_v1.rs`; `scripts/drills/adr0176_stage1_measure.py`; plan doc | dormant — this round; values from measurement |
| **ユーザー判断 2026-10-15** (bond式の再設計) | `palw_bond_formula_v2.rs` (fence `palw_bond_formula_v2`), hooks listed in the ADR amendment, RPC claim row v5 `reservationSplit` | dormant — this round |

### ADR-0176 cap coverage per path

"Reserve" = at acceptance; "consume" = at block / payout / Final; Q/B/R/F as reserved. Mode = the hold the claim takes.

| Path | Reserve | Consume | Mode (hold) | Notes |
|---|---|---|---|---|
| V2 Attempt (lead, merged) | `plan_attempt_budget_v1` Q 1, B 1 block, R escrow, F Final contribution, K liability (formula total past `palw_bond_formula_v2`) | B at once; R moved to kind-0 income hold, Final reward capped at it; F clipped at Final | Panel=1 | the block's uncarved worker base (`worker_base − carve`, 0 at t12's 720 ‰ carve) and tx-fee shares are per-block subsidy, paid even to a budget-refused attempt block → **user decision** whether R_total includes them (needs a coinbase change) |
| Riders / batches | Q 1, B its share of the lead's block, R its escrow, F | as Attempt; lead's block re-attributed in the same block (shadows follow) | Panel=1 | — |
| Free Prompt | Q 1, B quanta blocks, R quanta carves, F quanta weight | each receipt spend: B strict, R carve strict, F clipped | Panel=1 | receipt block pays the full worker base (carve = base at t12) — same decision as above |
| Kernel route, Panel-licensed | Q 1, R job escrow (user-paid), B = F = 0; producer liability = formula total past the fence | Final reward capped by kernel caps | Panel=1 | user escrow never clipped by the model budget: RFC-0016 P4 (`eee1710ce`) merged; `preview_claim_for_origin_moded_v1` keeps it and adds the hold admission |
| Kernel route, OPV (Panel=0) | as above | as above | Panel=0 | refused unbudgeted where the OPV policy requires a budget |
| Native funded v11/v13, stage v14/v28, text29 | Q 1, liability (class reserve, ≥ formula total past the fence), R held in native income rows | income payout through the income rows | Panel=0 | — |
| EXEC slices / EXEC_TX | slices reserve nothing new | paid from the root claim's R (kind-2 income hold); weight and DAA stay 0 | (root's) | `consume_block_for_bond` (H-3) has no caller: no EXEC block is reward-bearing |
| Round rights / fee-only Rounds | `reserve_round_rights_v1` (ExecutionCap T, 7-day long view) | used at the draw | — | per-day Round-rights rate = user decision (no day view for T) |
| Income rows (market fee, Round fee, source fee, native income, reporter) | R only | at payment, held `payment + W` | long view | per-day income rate = user decision |
| Final weight readers | fold `bond_budget_final_weight_v1`; **rule E** `PalwRuleEBudgetEngineV1` | — | — | the leaf v2 window and the node's sides read the same engine |
| Allocation (ADR-0177) | model R clip (coinbase-derived origins only, RFC-0016 P4) + **per-model reward-bearing block rights** (this round); Registered / Lapsed models excluded past `palw_model_eligibility_v1` (RFC-0017 GAP-E1, this round) | — | — | P_m once on blocks (RFC-0016 V+ when armed, which refuses this `block_opportunity`) and once on the coinbase (lead decision; C4 resolved) |

## Other ADRs — decision items

### ADR-0162 — The pair opens on a virtual reserve
| Item | Code | Status |
|---|---|---|
| D1 market open from creation | `consensus/core/src/palw_model_market_v1.rs:672`; `palw_state_v2/chain_state.rs:399` | dormant (`palw_model_virtual_v1`) |
| D2 V = 10 M BILI, stored per row | `palw_model_market_v1.rs:53`, `:354` | dormant |
| D3 pledge opens as seed | `palw_model_market_v1.rs:377` | dormant |
| D4 seed optional, before first trade (reason 14) | `palw_model_market_v1.rs:652`, `:73`; `palw_state_v2.rs:19520`; `misaka/evm/core/src/model_market.rs:254` | dormant |
| D5 buys only while Active | `palw_state_v2/fold_read.rs:758` | dormant |
| D6 buyback only into a trading pair | `fold_read.rs:787` | dormant |
| D7 fees unchanged; registrant fee 5 % past `palw_model_leg_v2` (user: registrant 5 %) | `palw_model_market_v1.rs:80/83/89/108` | live |
| D8 fence, collapse, fork id | `config/params.rs:2468`, `:4956`; `fork_id_v1.rs:1039` | dormant |
| D9 RPC / EVM / CLI / web | `misaka/evm/core/src/model_market.rs:655`; `misaka/apps/cli/src/palw_model.rs:63`; web display BILI this round | dormant |

### ADR-0163 — Adapter class
Items 1–5 (tag 94 listing, signed acceptance, fold refusals / delta 99, seat possession via the registry, fence
`palw_adapter_class_v1`): `palw_adapter_class_v1.rs:94/100/108/125`, `palw_adapter_class_fold_v1.rs:20`, `params.rs:3194` — **live
on t12 from 5,300**. Amendment 2026-10-07 (public prosecutability of adapters): docs-only future gate.

### ADR-0165 — Floor reserve, work carries the clock
A″ (`palw_real_share_v1.rs:121`, `:192`; refusal `fold_read.rs:921`), B (`palw_real_share_v1.rs:409`, `palw_heartbeat_v1.rs:350`),
constants (`:382`), RPC `blockKind` (`rpc/service/src/converter/consensus.rs:712`): **live on t12 from 5,300**. FALLBACK-V1 and
fallback accounting: **withdrawn** (no code, correctly). §10.2–10.4: research only. Heartbeat measurement for ADR-0176:
`scripts/drills/adr0176_stage1_measure.py` (this round).

### ADR-0166 — Verifier unavailability is not producer fraud
C (`palw_state_v2.rs:4553`, `deadlines.rs:348/405/411`), D (`state_params.rs:1652`), E (`palw_seat_availability_v1.rs:47/117`): live
on t12 from 5,300. 2026-10-12 amendment (no early reuse of consumed capacity after `PanelUnavailable`): **dormant — this round**
— the budget releases only at `d + W` whatever the void reason; past `palw_bond_formula_v2` the issuance slot is held to `d + W_M`
(`palw_issuance_slots_v1.rs`). Collateral is still released at once (unavailability is not fraud). §6 standby seats (phase 2):
**missing — design not specified** (no decision items to implement).

### ADR-0167 — ×1000 capacity package
D1 F-EM (`palw_capacity_s567_v1.rs:67/83`, `palw_admission_v2.rs:808`), D2 F-M1 riders, §3 F-K breaker, §4 ρ 250/1000 steps, §7
verification supply: **live on t12**. §6 M-0a/M-0b (fold cloning, root re-hash costs): performance work, not built. 2026-10-10 per-bond
preservation: ADR-0176 (dormant).

### ADR-0168 — Execution block (third class)
v1 design (`palw_exec_class_v1`, PXA1): **superseded** by the RFC-8 v2 integration (`palw_exec_v2_anchor.rs`, dormant
`palw_exec_payload_v2`). D1/D6 zero weight and DAA, D3/D4/D7 anchor (`palw_exec_v2_anchor.rs:35/41/112/348`), D5
(`palw_exec_v2_fold.rs:668/1133`): dormant. D8 (missing anchor heads as missing parents): **unverified** — evidence-only.

### ADR-0169 — Work slice as a normal consensus block
**Superseded direction** (2026-10-08). Nothing of it remains to implement; the replacement is the RFC-8 v2 work-slice ledger
(`palw_work_slice_v2.rs:1/588`, dormant). Its historical test record stays.

### ADR-0170 — The seed anchor is a window
M1–M3 and the fence (`palw_anchor_window_v1.rs:47–101`, `state_params.rs:749/755`, `transition_builder.rs:5923/6957/7064`,
`params.rs:3244`): **live on t12 from 5,300**; complete.

### ADR-0171 — Probabilistic constraint checks, court on dispute
R1 plan registration (`misaka/palw/kernel/src/plan.rs:82`), R2 Freivalds (`verify.rs:550`, `claim_compact_v1.rs:1096`), R3–R6
(beacon bootstrap, kernel receipts, court scope): partial, dormant; fence `palw_probabilistic_constraints_v1` refused
(`palw_probabilistic_constraints_v1.rs:21/26`). §5 soundness review: evidence-only.

### ADR-0172 — Versioned kernels
No universal VM (absence); versioned kernel + declarative plan (`descriptor.rs:54`, `plan.rs:82`, kernel route) partial dormant;
`KERNEL_EXTENSION_REQUIRED` fail-closed lowering (`misaka/palw/tir/lower/src/frontend_pack/mod.rs:214`): implemented off-chain.
Docs otherwise.

### ADR-0173 (seat root) — A seat holds a root
R1–R6 (`chain_state.rs:1140/715/1158`, `palw_class_seating_v1.rs:245/304`, `palw_state_v2.rs:720/20082/20152`,
`palw_improve_fold_v1.rs:1666`): dormant (`palw_audit_1004_v1`, int-13 list not armed). R5 largely moot under ADR-0175.
§2 activation / revoke skeleton (`palw_root_activation_v1.rs`): unwired. §7 six open questions: **user decision**.

### ADR-0173 (purpose) — Public-verifier dispute completeness
D1–D4 partial dormant (`palw_legacy_public_filer_v1.rs`, `palw_court_scope_v1.rs`); D5/D6/D8 docs gates (G14); D7 Panel=0
dormant (`palw_panel_free_v1`); D9–D11 revised by ADR-0177; D12 = ADR-0176 (dormant, this round's work).

### ADR-0174 — Token name Misaka, ticker BILI (naming-only)
Wallet suffixes (`wallet/core/src/utils.rs:53–60`), validator aliases, CLI value names: live. **This round:** the misaka-options web
app's user-visible "MSK" → "BILI" (`web/misaka-options/app.js`, 204 labels); kept: the EVM `nativeCurrency` entry and the
`0x4D534B` chain-id mnemonic (compatibility identifiers), RPC field names (`balanceMsk`, by design). What remains: nothing in code.

### ADR-0175 — Registered models are immutable
Refusal code complete and dormant (`palw_model_immutable_v1`, `params.rs:2419`; `palw_lifecycle_objects_v2.rs:840`,
`palw_state_v2.rs:18269/18276/17798`, improve fold, onboarding, CLI). No code gap; activation = user decision.

### ADR-0177 — Model-bond allocation without availability consensus
| Item | Code | Status |
|---|---|---|
| D1 Torrent / Seeder withdrawn; public material non-consensus | absence; `palw_public_material_v1.rs:3` | implemented |
| D2 court scope: claim evidence only | `palw_court_scope_v1.rs:217/278/379` | dormant |
| D3 `S_m`, tag 140, seasoning, pro-rata | `palw_bond_budget_v1.rs` (assign, pro-rata), **V3 snapshot `allocation_v3.rs:135`** | dormant |
| D4 `A_m = S_m^α`, α = 1.5 (decided 2026-10-14) | curve; **V3 validation pins α = 1.5** | dormant — this round |
| D5 model budget + per-bond caps, caps never raised by allocation | caps read only capital | dormant |
| 2026-10-14: miner + verifier capital, 30-day average | **`roll_epoch_v3` `allocation_v3.rs:173`**, fold `bond_budget_verifier_capital_v1` | dormant — this round |
| 2026-10-14: P_m for block opportunity and coinbase, applied once | **`model_blocks_available_v1` `allocation_v3.rs:264`**, fold plan/commit (reward-bearing block rights per model: the attempt block itself still exists, only its claim is refused — RFC-0016's "B+" level; the physical layer of RFC-0016 D-Q1 is Draft and not implemented); coinbase = existing R share | dormant — this round; single application = decision C4 |
| 2026-10-14: no registrant/owner coinbase leg | allocation pays none | implemented |
| D6/D7 economic evaluation, effective `p` | — | evidence-only |
| μ (1.3–2×) | — | kept for comparison only (user) |

### ADR-0178 — Fork choice heals a partition on bonded participation (rule E)
Items 1–6 (`palw_fork_choice_rule_e_v1.rs:62–723`, `consensus/src/pipeline/virtual_processor/palw_rule_e.rs`, relay/IBD flows):
dormant, refused. Leaf v2: partial (L2FC envelope pending). **§10 budget interface: wired this round** (`PalwRuleEBudgetEngineV1`,
granted Final weight, `⌊accepted/W⌋` interval, `F_max` at the bond's collateral). §11 blockers (L2FC, multi-node E2E, `W_p`,
heights): user decision / evidence-only.

## Compile risks of this round (unbuilt)

* `PalwBondBudgetPolicyV1` and `PalwModelAllocationPolicyV1` lost their derived Borsh (manual impls, V2 bytes identical) and
  gained `hold` / `average`: every full struct literal was given the field (scripted, then checked); a literal outside
  `consensus/` / `kaspad/` would fail.
* New enum variants: `PalwBudgetOriginV1::{HoldDay, HoldBurst, HoldLiability}`, `PalwBudgetRefusalV1::{Burst, ModelBlockOpportunity}`
  (no exhaustive external match was found).
* `Params::palw_bond_formula_v2`: added to the struct, the clone-construct, four preset literals, the three exhaustive
  destructures and both hashers in `params/identity.rs`, the fork-id probe, validation and the A-2 table.
* `PalwStateParamsV2` gained a `#[borsh(skip)]` mirror; `PalwClaimRowV1` gained `reservation_split` (four literals updated,
  including two kaspad tests); `RpcPalwClaimRow` gained `reservation_split` (version 5, proto field 38, both grpc converters,
  the v2-truncation test's tail).
* `palw_rule_e_bond_budget_v1` now returns `PalwRuleEBudgetEngineV1<'_>` (both callers take `&`).
* Fold: `PalwBudgetPlanV1.mode`; closures in `tick_bond_budget_v1` pass one `capital_of` closure to either roll.
* Test sources: `consensus/core/tests/adr0176_mode_hold_v1.rs`, `adr0176_bond_formula_v2.rs`, `adr0176_cap_sweep.rs`,
  `adr0177_allocation_v3.rs`, and one test appended to `palw_bond_budget_e2e.rs`.
