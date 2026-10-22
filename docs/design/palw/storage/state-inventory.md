# PALW state と node store の在庫 — 30 GB pruned node のための分類 (lane storage/state, 2026-10-22)

Status: **設計メモ（採用・有効化ではない）**。基準 commit b1eb33960。consensus に影響する部分は [RFC-0036](../../../rfc/0036-authenticated-palw-state-map-and-row-access.md) / [ADR-0203](../../../adr/0203-authenticated-palw-state-map-and-row-access.md)（いずれも Proposed / Review Required）に分離した。実装は dormant な純ライブラリだけで、どの network の state root・判定も変えない。

目的: pruned node が合計 30,000,000,000 bytes 以内で IBD・追従・再起動・reorg を完了するために、(a) 再生成可能な履歴と (b) 現在必要な state を分け、(b) を「検証済み row を必要時に取得」へ移すための前提を固定する。数値は実測（live t12 node1、DAA ≈13.7k、合計 8.5 GB: consensus 7.5 GB、palw-retention 920 MB）以外は推定駆動要因だけを書く。

凡例:

- **永続** = 意味上削除できない（登録・使用済み鍵・消費済み offence など）。pruned node でも「存在しない」と答えてはならない。
- **責任** = 未解決の claim / court / DA / 支払の責任材料。解決まで削除禁止。解決後は回収可能。
- **一時** = 次 block / 短い窓で消える。
- **派生** = 他の rooted row から再構築できる（root に入らない索引を含む）。

## 1. PalwChainStateV2 の rooted family

正本: `consensus/core/src/palw_state_v2.rs` の `PalwChainStateV2`（約 150 field、全 private）。書き込みは `TransitionBuilder::write_*`（約 115 個）だけで、各書き込みは `PalwDeltaEntryV2`（key・old・new）を 1 件積む。

| family | key → value | 増加要因 | 削除 | 削除禁止条件 | サイズ駆動 |
|---|---|---|---|---|---|
| `bonds` | outpoint → bond（ML-DSA-87 公開鍵 2,592 B を含む） | `BondRegistered` | **永続**（status は上書き、`write_bond(_, None)` は delta undo のみ） | 登録と使用済み公開鍵の意味（`bond_of_pubkey_v2` は bonds 走査） | bond 数 × ≈2.7 KB |
| `reserved_exposure` / `registration_exposure` | bond → u128 | claim / 登録 | 0 で消える（派生的） | 未解決 claim の予約中 | bond 数 |
| `classes` ほか class 系 7 表、`class_step_ladders` / `court_windows` / `tir_classes` / `gen_classes` / shard plans / `fp_work_profiles` / `class_semantics` | class → record | `ClassRegistered`、epoch 境界 | **永続**（`class_walks` に 1 削除経路） | モデル登録は永続（PRINCIPLES §9） | class 数（年次指定 + 登録） |
| `capabilities` | H(class, bond) → fact | (class, bond) 初回 claim | **永続** | — | 最悪 classes × bonds（コード注記どおり） |
| `seat_readiness` / `seat_root_readiness` / `model_lifecycles` | (bond, class[, root]) | readiness 証明・registry | **永続** | — | bonds × classes |
| `certified_families` / `fp_certified_*` | digest → row | 認証 | **永続** | — | family 数 |
| `consumed_offences` | offence → row | 有罪確定ごと | **永続** | 二重処罰防止 | 有罪数 |
| `excluded_auditors` / `bond_freezes`（final） | bond → … | 有罪 | **永続**（tier freeze は解除） | — | 有罪数 |
| `activation_pools` / `artifact_owners` / `model_lines` / `model_benefits` | class / root / line | 購入・公開 | **永続** | — | 登録数 |
| `claims` | claim → ClaimState | attempt / FP claim | **責任**: Final/Void + 退役遅延後 `sweep_deadlines` → `retire_claim` | 未解決 claim | 同時生存 claim 数 |
| claim 付随 13 表（`panels`, `panel_duties`, `claim_roots`, `claim_economics`, `claim_semantics`, `shard_licensing`, `tir_shard_claims`, `held_da_missing`, `held_leaf_demands`, `audit_receipts`, `derived_artifacts`, `fp_claim_prompts`, `fp_public_jobs`） | claim スコープ | panel 束縛・claim | **責任**: claim 終端 / 退役で削除 | 同上 | 同上 |
| `court_sessions` / `court_close_groups` / `tir_dissections` | session | challenge | **責任**: 解決時（`write_court(None)` 8 箇所） | 未解決 court | 同時 court 数 |
| `slashable_locks` / `panel_liabilities` | (seat, claim) / claim | Valid seat / Final・void | **責任**: epoch 境界で期限 + 第二時計の後に prune | lock 寿命内 | seat × 生存 claim |
| `da_sessions` / `da_claims` | (claim, accuser) | DA 告発 | **責任** | 未解決 DA | 告発数 |
| `reward_pending` / `reporter_commitments` / `reporter_rewards` / `held_forfeits` | offence / slot | R-3/R-4 | 責任または一時（reveal 窓、`window_court`） | 窓内 | 有罪数 |
| `vesting` | claim → row | Final 報酬 | **責任**: 支払または burn で削除（claim 退役より長生き） | 未払い | Final 数 × vesting 期間 |
| `legacy_disputes` / `staged_courts` / `terminal_evidence` / `legacy_answers` | claim / staged key | 公開 court 経路 | **責任**: 期限 sweep | 未解決 | staged 数 |
| `funded_escrows` / `exec_v2`（roots, slices, jobs, anchored, rewards, holds, origins, job_uses） | contract / root / job | escrow・EXEC | **責任**: 保持期間後に退役（`funded_escrow.rs:138`, `palw_exec_v2_fold.rs:1368`） | 未精算 | 生存 job 数 |
| `pending_payouts` | claim → payout | 前 block の Final | 一時（block 頭で ≤ `PALW_V2_MAX_PAYOUTS_PER_BLOCK` 消化） | 未払い | Final/block |
| `evm_settlements` / `activation_pool_scheduled` | seq | EVM action | 一時（次 block） | — | 小 |
| `pending_chunks` | group | 分割 object | 一時（TTL） | — | 小 |
| round 系（finals, schedules, pending, permits_used, equivocations） | span | 実行 lane | 一時（2 span） | — | 小 |
| `operator_ring`, `recent_anchor_daas`, `withholding_strikes`, `network_demand`, `capacity_ledger`, `issuance_buckets` | daa / bond | block / admission | 環・期限（`issuance_buckets` は bond ごとに残る） | — | bond 数 |
| `model_markets` / `model_positions` / `model_versions` / proposals / evaluations | line / (line, holder) | market | position は 0 で削除、version は直近 N | — | 保有者数 |
| `improvement_*`（21 表） | (line, epoch, …) | RFC-0004 | epoch 退役 | mainnet 後へ延期（PRINCIPLES §12） | — |
| vertex / mesh | round / claim | RFC-0007 | block ごと sweep | — | 小 |
| **`kernel_route`**: header + `rows: LedgerRowsV1` + **`aux: BTreeMap<(u8, Vec<u8>), Vec<u8>>`** | aux table id 32〜145 の約 110 表（32 bond keys, 33 assignments, 34 receipts, 35 block budget, 41 chunk groups, 46〜106 model/flat/execution, 140 settlement-once, 142 paid-claim trap, 144 claim-local / liability hold, 145 receipt pause ほか） | kernel claim・receipt・登録 | 表ごと（登録系は永続、liability hold は責任） | 表ごと | aux 行数（ledger 全体は flush ごとに全行 diff） |
| `panel_v3`, `bond_budget`, `tir_designated`, `d2_readiness` | engine 下位 state | | 各自の表 | 各自 | |
| scalar（`safe_weight`, `retired_safe_weight`, `settled_attempt_finals`, `bounded_immature`, `safe_frontier*`, `last_point`, `panel_reserve_sompi`, `round_span`, `epoch_budgets`, `floor_state`, `work_target`, `round_seed_anchor`, counters） | — | — | — | — | 定数 |

root に入らないもの: `work_target_shadow`, `final_work`, `exec_v2_reward_payees`、約 50 の派生索引（`deadlines`, `unresolved`, `court_deadlines`, `da_deadlines`, `vesting_order`, `staged_*`, `improvement_due`, `capacity_weight_index` ほか、`palw_state_v2.rs:11163-11290`）、`kernel_route.ledger_cache`（`#[borsh(skip)]`）。派生索引は decode と delta 適用のたびに**全件再構築**される。

### state root の現在形（変更しない）

`state_root()` = keyed BLAKE2b(`PALW_STATE_V2_DOMAIN_STATE_ROOT`, preimage)。preimage は `state_root_preimage_located_v1`（`chain_state.rs:2791-3250`）が固定順で書く: version → bonds → … → receipt_epoch_counters（常在 23 表）→ 非空時だけの block 群（model market … kernel-route/v1 … registry_probation）→ scalar と残りの engine root。各表は `palw_collection_root_of_entries_v1(label, len, 全行を順に)` で、**表全体を 1 本の逐次 hash** にする。例外: kernel route は `ledger_root()` と `aux-root/v1`（`(u8 table, u32 klen, key, u32 vlen, row)` の逐次 keyed hash）、`tir_classes` / `gen_classes` / `panel_v3` / `bond_budget` は各自の root 関数。どれも行単位の証明を持たない（[palw_state_proof_v1.rs](../../../../consensus/core/src/palw_state_proof_v1.rs) は表の全行を開く）。

## 2. UTXO と EVM

| family | 形 | 永続/回収 | 備考 |
|---|---|---|---|
| UTXO set（VirtualUtxoset, PruningUtxoset） | outpoint → entry、現在値 | 現在値は検証に必須 | 未消費は意味上削除不可。pruning point の UTXO set は IBD staging と二重に存在する |
| utxo_diffs / utxo_multisets | block → diff / MuHash | pruning で回収 | (a) 再生成可能履歴 |
| EVM `EvmStateDiff`(206) | block → **完全な `EvmStateSnapshot`** | pruning で回収 | 「O(state) per block」とコード自身が注記（`evm/mod.rs:1482-1490`）。per-block 完全 copy の典型 |
| EVM `EvmStateDiffV2`(220) / checkpoint(221) | 正規 block ごとの前進 diff / checkpoint | archive mode 以外は回収 | |
| EVM `EvmFlatAccount`(234) / `EvmBlockStateRoot`(232) / `EvmLatestStatePtr`(231) | 現在 state / block root | 現在値必須 | |
| EVM `EvmCode`(222) | content-addressed | 回収しない | 参照される限り必要 |
| EVM payload / receipts / trace / logs / tx lookup | per block | pruning で回収 | (a) |

## 3. node store（`consensus/src/model/stores`）

| store | key → value | 範囲 | pruning |
|---|---|---|---|
| headers（+compact, compressed） | hash → header | per block、**大** | 回収（proof level を除く） |
| block_transactions | hash → body | per block、**大** | 回収 |
| utxo_diffs / acceptance_data | hash → … | per chain block、**大** | 回収 |
| utxo_set | outpoint → entry | 現在、**大** | — |
| ghostdag, relations, children, reachability, statuses, daa, depth, pruning_samples, rewarded_epochs, validator quality, reserve balance | hash → … | per block | 回収（relations は proof 用に一部保持） |
| selected_chain, tips, headers_selected_tip, virtual_state, dns_state, pruning*, past_pruning_points, pruning_overlay_snapshot | singleton / index | global | — |
| epoch_accumulator | epoch → tally | per epoch | **回収しない** |
| stake_bonds | outpoint → record | global | 回収しない |
| **palw_state_v2** | `PalwStateV2Deltas`: block → {root, `PalwStateDeltaV2`}（per chain block、pruning で削除）/ `PalwStateV2Tip`: sink の**完全 carriage**（resolve batch ごとに全書き換え）/ `PalwPruningPointState`: pruning point の完全 snapshot + native inventory | delta + 完全 snapshot 2 本 | delta は回収 |
| palw_class_carriage | class → carriage | append-only | 回収しない |
| palw_legacy_answer_history_v2 / palw_prefix_history_v1 | block / head | archive | DAA GC |

既に「完全 copy per block」ではない（delta + tip + pruning point）。問題は **tip の毎 resolve 全 decode・全索引再構築・全 root 再計算**（`load_tip_retained_v1`）と、下記 §5 の**メモリ上の全量 clone**、そして全件を手元に持つ前提そのものである。

## 4. 全表操作（per block / per decision）と分類

分類: **R** = 行 lookup（鍵付き行だけで足りる）、**I** = 索引による完全範囲（2 次索引の連続範囲で足りる）、**S** = 認証 stream（表全体の完全な順序走査が要る）、**D** = algorithm 再設計が要る（全行に依存し索引で解消しない／結果の意味が変わる → RFC 必須）。

| # | 場所 | 対象 | 契機 | 計算 | 分類 | 変換方針 |
|---|---|---|---|---|---|---|
| 1 | `palw_panel_v2.rs:1266` `palw_panel_bonds_judging_v1` | bonds (+ freezes, exposure) | panel draw ごと、PanelBound 検証ごと | 適格 filter → stake 加重 race | **S** | 取得した勝者 1 行は「より良い候補が隠されていない」を示さない。適格 bond 表の完全 stream（`AuthStreamVerifierV1`）が必要。stream 量を per-block 上限にするには抽選を「適格集合の累積 stake 木で O(log n) 降下」へ変える必要があり、**抽選結果が変わる → RFC-0036 §6 の別 RFC** |
| 2 | `palw_panel_v2.rs:2371` `PalwStakeDrawCensusV1::new` | bonds | SW-8 draw 点ごと | 適格 census | **S** | 1 と同じ census を共有 |
| 3 | `palw_state_v2.rs:18020` `palw_claims_provisional_past_their_anchor_slot_v1` | claims | 毎 block | Provisional かつ `bind_base + delay ≤ daa` | **I** | (deadline, claim) 索引を rooted にする |
| 4 | `palw_state_v2.rs:17968` `palw_sw8_thin_draws_v1` | claims × census | 毎 block（fence 後） | thin-draw 集合 | **S** | 1 に従属 |
| 5 | `palw_kernel_route_fold_v1.rs:1386` `draw_seats` | bonds | kernel assignment ごと | operator ごとの最小 hash-key、top-k | **S** | (operator, hash-key) 順の stream。top-k の早期打切りは別 RFC |
| 6 | `palw_kernel_route_fold_v1.rs:2333` `tick_kernel_route_v1` | ledger.bonds | 繁忙 block ごと | 全 bond の collateral 再同期 | **D** | 変化した bond だけを同期する差分規則（結果同一性の証明が要る）|
| 7 | `palw_kernel_route_fold_v1.rs:415, 770-778` `loaded_rows` / `flush` | ledger 全行 | flush ごと | `to_rows()` + `diff_rows` | **D** | ledger を行単位 store に（書いた行だけ diff） |
| 8 | `palw_kernel_route_fold_v1.rs:912-919` `finish_exec_rewards_v1` | ledger rows + bonds | EXEC finalize ごと | rows clone + 再同期 | **D** | 7 と同じ |
| 9 | `palw_model_eligibility_fold_v1.rs:492` `sweep_model_eligibility_v1` | bonds（線形 find） | 毎 block | kernel-id → bond | **R** | kernel-id 索引 |
| 10 | `chain_state.rs:1897` `bond_of_pubkey_v2` | bonds find | 登録検査 / RPC | pubkey → bond | **R** | 使用済み公開鍵の rooted 索引（永続） |
| 11 | `transition_builder.rs:81-99` `registration_index_v1` | 全 bonds | transition ごと（lazy） | key/operator 索引 | **R** | 索引を永続化 |
| 12 | `palw_rfc0016_fold_v1.rs:314` `rfc0016_capacity_read_v1` | bonds × model_lifecycles | admission gate / tick | class ごとの ready verifier 供給 | **D** | class ごとの増分カウンタを rooted に（同値性試験） |
| 13 | `palw_class_seating_v1.rs:218` `class_ready_operators_v1` | bonds (+ seat_readiness) | class seating 検査ごと | class の ready operator | **I** | class → bond 索引 |
| 14 | `palw_model_registry_v1.rs:1898, 1921, 1944, 1962` | bonds, claims | span ごと | class ごとの数 | **I/D** | class 別カウンタ |
| 15 | `transition_builder.rs:7105, 7152` `step_model_registry` | classes × claims（`any(class && Final)`） | span 境界 | class ごとの存在 | **I** | (class, phase) 索引 |
| 16 | `palw_state_v2.rs:17165` `apply_class_share_growth` | claims | epoch 境界 | 閉じた epoch の class 別 Final 数 | **I** | (epoch, class) カウンタ |
| 17 | `palw_state_v2.rs:17458` `apply_class_retargets` | classes | epoch 境界 | 全 class retarget | **S** | class 表 stream（class 数は小） |
| 18 | `palw_state_v2.rs:17402` `redistribute_permille_v1` | class_shares | 登録 / 回収 | largest-remainder | **S** | 同上 |
| 19 | `palw_state_v2.rs:~17278` `apply_class_reclamation` | class_shares + classes | epoch 境界 | Active class 走査 | **S** | 同上 |
| 20 | `palw_state_v2.rs:17006` `apply_final_lock_life_retro` | slashable_locks | 一回限りの fence 越え | 期限の書き換え | **S**（移行） | 一度だけの stream |
| 21 | `palw_state_v2.rs:17748` `sweep_panel_obligations` | slashable_locks, panel_liabilities | epoch 境界 | 期限 / 時計で prune | **I** | (expiry, key) 索引 |
| 22 | `palw_state_v2.rs:17693` `sweep_fp_prompt_rows` | fp_claim_prompts | 毎 block | `expires ≤ daa` | **I** | 同上 |
| 23 | `palw_state_v2.rs:18289` `sweep_reward_reveals` | reward_pending | 毎 block | `reveal_until < daa` | **I** | 同上 |
| 24 | `palw_state_v2.rs:18372` `sweep_reporter_commitments` | reward_pending × consumed_offences | 毎 block | accepted_daa の最大（guard） | **D** | 最大値の維持（rooted scalar） |
| 25 | `palw_state_v2/funded_escrow.rs:138` `sweep_funded_escrows_v1` | funded_escrows（2 回） | 毎 block | resolved / retirable | **I** | 状態別期限索引 |
| 26 | `palw_exec_v2_fold.rs:1368` `sweep_expired_roots_v2` | exec_v2.roots | 毎 block | `expiry ≤ daa` | **I** | 期限索引 |
| 27 | `palw_bond_budget_fold_v1.rs:2116` `tick_bond_budget_v1` | bond_budget.claims | 毎 block | 消化された payout と照合 | **I** | payout key 索引 |
| 28 | `palw_bond_budget_fold_v1.rs:99, 2426` `round_fee_admission_v1`, `verifier_capital` | bonds, panel_duties | fee admission / budget | capital map | **S** | bond 表 stream、または capital 合計の rooted 維持（D） |
| 29 | `palw_mesh_fold_v1.rs:719` `sweep_mesh_v1` | mesh audits / traps | 毎 block | `row_end < daa`（上限付き） | **I** | 期限索引 |
| 30 | `palw_legacy_answer_chunks_fold_v2.rs:128`, `palw_terminal_carriage_fold_v1.rs:137` `sweep` | legacy_answers, terminal_evidence | 毎 block | 期限 **または** 行間述語 | **I/D** | 期限部分は索引、行間述語は個別評価 |
| 31 | `palw_court_capacity_fold_v1.rs:280, 392, 743` | legacy_disputes × courts, court_sessions | court 操作 / 毎 block | pool 集計、終了・期限 | **D/I** | pool 集計を rooted カウンタに |
| 32 | `transition_builder.rs:10036, 10282`; `objects.rs:329, 421, 656, 868, 976`; `palw_held_close_fold_v1.rs:88`; `palw_memory_routing_fold_v2.rs:86` | court_sessions `find/filter(claim == id)` | object / claim 終端ごと | claim の session | **R** | claim → session 索引 |
| 33 | `transition_builder.rs:4388`; `palw_g14_lane_closure_fold_v1.rs:183` | slashable_locks の鍵 | Final / void ごと | claim の lock（鍵は (seat, claim)） | **I** | (claim, seat) 索引 |
| 34 | `transition_builder.rs:3437, 3442` `forfeit_bond_v1` | vesting | 没収ごと | producer / seat 別の行 | **I** | (bond, claim) 索引 |
| 35 | `palw_state_v2/fold_read.rs:1788` `check_network_room_v1` | 全 claims + network_demand | attempt admission ごと | bond ごとの未 licence 数 | **D** | bond 別カウンタ |
| 36 | `palw_issuance_slots_v1.rs:190` `palw_issuance_read_at_v1` | claims | attempt ごと | bond の slot 保有 claim | **I** | (bond, claim) 索引 |
| 37 | `palw_audit_door_v1.rs:253, 259` `audit_backlog` / `audit_overdue` | claims | attempt ごと、rho-breaker epoch | 数 | **D/I** | カウンタ / 期限索引 |
| 38 | `palw_security_certification_v1.rs:335` `palw_gate_c_frontier_v1` | claims | Gate C 後の毎 block | Final の最大 accepted_blue_score | **I** | (score, claim) 索引の末尾 |
| 39 | `palw_state_v2.rs:13542` `palw_inflight_index_build_v1` | unresolved 索引 + claims | transition ごと（lazy） | class 別 tally | **I** | |
| 40 | `palw_state_v2.rs:22853, 22889, 23003`; `palw_panel_v3_fold_v1.rs:1180` `rebuild_*_indices`, `refresh_derived_v1` | claims, courts, 全体 | **毎** `apply_delta_v2` / `revert_delta_v2` と import | 全索引再構築 | **D** | 索引を rooted / 永続にし delta で増分更新 |
| 41 | `palw_state_v2.rs:15365-15380` | court_sessions, claims | 毎 block（debug build のみ） | 等価検査 | — | release に影響なし |
| 42 | `chain_state.rs:2773` `state_root()` | **全表** | 毎 chain block、毎 tip load | 全量再 hash | **D** | 増分 commitment = RFC-0036 の認証 map（新 root 版、fence 後） |
| 43 | `palw_fork_choice_rule_e_leaf_v3.rs:75-118` | bonds（整列、tree root、Σ capital） | v3 leaf が支配する chain block | registry root | **S** | bond 表の認証 map root をそのまま使える（別 RFC で同値性） |
| 44 | `palw_fork_choice_commitment_v1.rs:166` | bonds `.count()` | chain block | 件数 | **R** | 表 root が count を commit する |
| 45 | `palw_rfc0018_v1.rs:417, 423`; `palw_fork_choice_rule_e_v1.rs:639, 774` | 2 fork の bonds | fork choice 判定 | merge-join、和 | **S** | 2 root に対する範囲 stream の merge |
| 46 | `consensus/src/.../processor/palw.rs:595` `palw_frontier_provenance_outcome_checked` | challenger の bonds − 祖先 | reorg 判定 | fork 上で mint された bond | **I** | registered_daa 索引 |

既に順序付き索引で期限到来行だけに触れているもの（変換は「索引を rooted にする」だけ）: `deadlines.rs:297` / `:5`、`palw_state_v2.rs:18150`（`da_deadlines`）、`palw_legacy_public_filer_fold_v1.rs:709`、`palw_staged_court_fold_v1.rs:3895`（`staged_due`、≤ 64/block）、`palw_improve_fold_v1.rs:1300`、block 頭の payout 消化。ただしこれらの索引は永続化されず、decode / delta のたびに全件再構築される（#40）。

**集計**（46 件）: R 5、I 17、S 11、D 8、混在（I/D・D/I）4、debug のみ 1。D と「S を per-block 上限内に収める変更」はすべて判定結果・consensus 意味に触れうるので、storage 変更の中に隠さず RFC に分ける（RFC-0036 §6 に一覧）。R と I は「rooted 索引を追加し、同じ結果を返す」ことを試験で示せば意味不変。

## 5. 親 state の全量 copy

| 経路 | 何を copy するか | 頻度 |
|---|---|---|
| `TransitionBuilder::try_new`（`palw_state_v2/transition_builder.rs:35-44`）→ `NativeTransitionReservationV1::copy_parent_v1`（`palw_native_resource_v1/transition.rs:20-31`）→ `NativeRetainedV1::try_from_clone_v1`（`retained.rs:119-130`） | 親 `PalwChainStateV2` 全体を `clone()` | transition ごと。入口は `apply_palw_transition_v7_inner`（`palw_state_v2.rs:14691`）、`palw_v2_pre_object_base_retained_v1`（`:14260`）、`palw_v2_apply_one_object_retained_v1`（`:14310`） |
| node: `calculate_utxo_state_relatively`（`virtual_processor/processor.rs:1885`） | acceptance filter の pre-object base（1 clone、`palw_accept.rs:684/705/713`）、**carried object ごとに 1 clone**（`:1596`, `:1842`）、本 fold でもう 1 clone（`:2706`）、`*state = next` 後に全 root 再計算 | chain block ごとに (2 + object 数) 回 |
| `try_checkpoint` / `restore`（`transition_builder.rs:8978-8984`） | 作業中 state 全体を rollback 用に clone | 33 箇所: merged attempt ごと（`palw_state_v2.rs:15075`）、receipt spend ごと（`:15136`）、rider batch、court 系、bond budget、TIR、kernel route、EXEC など |
| kernel route | `route.rows.clone()`（ledger 全体、`palw_kernel_route_fold_v1.rs:888, 915, 945`）、cold cache では rows から ledger 再構築、flush ごとに全 `to_rows()` + `diff_rows` | EXEC / flush ごと |
| reorg / walk | `apply_delta_v2` / `revert_delta_v2` がそれぞれ `parent.clone()` / `child.clone()` + 全索引再構築（`palw_state_v2.rs:22232-22263`） | walk の block ごと（`palw_state_walk.rs:93, 97`、`processor.rs:1946, 2062`、native reader `palw.rs:4137, 4192`） |
| tip I/O | `load_tip_retained_v1` が毎 resolve で全 decode・再検証、batch ごとに全 serialize（`processor.rs:2922`）、pruning point snapshot も全書き | resolve / batch ごと |

既存の差分型: `PalwStateDeltaV2 { point, entries }`（約 131 variant、各 key・old・new。apply / revert は old を検証）、kernel route の行単位 journal `(table, key, old, new)`、`misaka_palw_kernel::rows::diff_rows`、EVM `EvmStateDiffV2`。copy-on-write や構造共有 map はない（全 collection が `BTreeMap`）。

### change-set への変換計画（意味不変の段階と、fence が要る段階を分ける）

1. **読み取りの seam（意味不変、今回 dormant 実装済み）**: `PalwRowSourceV1`（親 root に pin した行読み、unavailable ≠ absent）と `PalwFullStateRowSourceV1`（現 state そのまま、既存 fixture 上で行・probe・範囲が同一であることを試験）。consensus 経路は未接続。
2. **書き込みを overlay に（意味不変）**: `TransitionBuilder` の `write_*` は既に old/new を `PalwDeltaEntryV2` に積んでいる。親を clone する代わりに「親への参照 + `PalwRowOverlayV1`（変更だけ）」を持たせ、読みは overlay → 親の順にする。`try_checkpoint` / `restore` は overlay の長さの savepoint（journal を巻き戻す）に置換し、全量 clone をなくす。delta は overlay の変更集合から生成する（`PalwRowChangeSetV1` は old を pin 下の読みで固定する）。完了条件: 全 fixture と live replay で delta と state root がバイト同一。
3. **派生索引を増分化（意味不変）**: #40 の全索引再構築を、delta entry ごとの索引更新に置換。reorg は inverse change set（`PalwRowChangeSetV1::inverse`）で戻す。
4. **node の clone 削減（意味不変）**: object ごとの rehearsal を overlay の savepoint で行い、acceptance filter も同じ overlay を共有する。tip は「最後の変更集合 + 定期 snapshot」で持ち、毎 resolve の全 decode をやめる。
5. **行の取得元を切替（fence 後、RFC-0036 採択が前提）**: overlay の下の親を `PalwProvenRowSourceV1`（認証 map の行証明を pin で検証）に替える。state root は新形式（directory + header）となるため、新 root 版として `palw_state_row_map_v1` の後だけ。供給者の不在は sync 停止であって block 無効ではない。
6. **全表操作の解消**: R/I は rooted 索引（新形式の表として追加）、S は `AuthStreamVerifierV1` による完全 stream、D は個別 RFC（抽選・集計規則の変更を伴うもの）。

段階 2〜4 は consensus 意味を変えない最適化として進められる（PRINCIPLES §11 の「意味不変の実装・最適化」）。段階 5〜6 は RFC-0036 と後続 RFC の採択後に限る。

## 6. 今回の実装（dormant）

- `consensus/core/src/palw_state_auth_map_v1.rs`（+ `bits` / `trie` / `proof` / `state` / `tests`）: 認証付き順序 map と証明。
- `consensus/core/src/palw_state_row_access_v1.rs`（+ `tests`）: 行アクセス trait、overlay / change set、bounded cache、full-state adapter、proven backend、fence gate。
- `consensus/core/src/palw_state_v2/tests/row_access_v1.rs`: 既存 fixture 8 種での適合試験。
- `Params::palw_state_row_map_v1`: 全 preset `None`、Some-only hash、武装は `validate_palw_state_row_map_v1` が拒否。
