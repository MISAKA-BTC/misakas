# int-10: the model court window — review (2026-10-01)

Subject: Codex's `palw_model_court_window` (`codex/t12-court-window`, uncommitted state of 2026-10-01 09:40), applied to `rcore/int-10`
beside `palw_tir_fence2` in one flag-day list at DAA 3,600. Past the fence a fused-attention graph class or an IR class with a dissected
cone commits at registration `w = max(network window, W(shape))`, `W = (2·(L+H) + t + 1)·turn + assembly reserve + 1` (`L` the leaf
ladder's rounds, `H` the history dissection's, `t` the terminal moves). It is stored in `class_court_windows` (root block, carriage tail
`0xE0`, delta 90); every court opened for that class takes `w` as its backstop and extends the claim's retention to its deadline.

**Premise check (read first).** testnet-12 arms the held fence from genesis and admission charges the NETWORK's regime, so no class's window
carries the ladder's rounds. At t12's court (turn 42, ladder 40, terminal 2, arity 4, reserve 1,616) `W` is **≤ 3,000 for every history up
to 2^32 positions at tile 16** (2,919 at 2^32; 3,591 at 2^48; 4,263 at `u64::MAX`), so on t12 `w` is the network window for every realistic
class and the flag day changes no admission; what it changes is state (a row per class registered past 3,600) and the retention rule (D1).
The "5,102 > 3,000" figure is the ladder clock (83 moves × 42 + 1,616: the fit tool's Shipped regime, a network without the held fence),
which t12 never charges. If that example was measured on t12, this fence buys nothing there and fence2 alone would do.
(`palw_t12_flag_day_3600::a_models_window_…` prints the table.)

| # | Check | Finding |
| --- | --- | --- |
| 1 | Determinism, overflow | One derivation serves admission, the acceptance walk and the fold, on the court the ruleset derives at the registration DAA. All `checked_*`; `L`,`H` ≤ 64 (bounded loops), so `W` ≤ 10,479 at t12 whatever the shape; overflow or a zero tile is refused, never saturated or sent to a shared window. The fold reads `extras.class_court_windows`, a pure function of (objects, DAA, params): the 7 processor sites that fold objects pass it, the other 24 fold none; a missing row fails closed (`ClassCourtWindowMissing`). |
| 2 | Reorg | `ClassCourtWindow{key,old,new}` through the same `swap_write!` as every table; written after its class row, reverted before it; discriminant 90 pinned; round trip tested. A class is never removed. |
| 3 | IBD, carriage | Tail and root block only when non-empty; load checks each row has a class and is ≥ the network window. Below the fence both are int-8's byte for byte; int-8 cannot decode a post-fence carriage and is refused earlier by the fork id. The dormant sync walk (`palw_state_v2_sync.rs`, constructed nowhere) must carry the fence if ever wired. |
| 4 | Legacy classes | Unchanged below and above the fence after D1/D2: no row, network window, the release's retention bound. |
| 5 | fence2's DA ladder | Orthogonal: it bounds the step LEAVES of an IR DA answer, the window bounds the TIME of a session. They meet in `claim.trace_retention_daa` (DA demands check `deadline > retention`; a model-window court extends it). Re-checked on lane A's final sha. |
| 6 | Before / after | Before: 3,000, and a shape that needs more is refused. After: `w` per class, once, never rewritten; a class registered below keeps 3,000 for life. |
| 7 | kaspad | The janitor and the seat horizon read retention from the chain, so an extension is seen if read after the opening; a seat's noted liability is not refreshed (R3). Panel: D5. |

**Defects.** **D1 (fork risk).** The retention rule (refuse only an opening past retention; extend retention to the deadline) applied to
every class and below the fence: a court int-8 refuses (`deadline > retention`) would open on int-10 and rewrite the claim, a split during
the rollout; it also failed `court_outside_retention_is_refused_past_the_deep_fence`. Now gated on the class having a row; both halves tested.
**D2.** The window derivation asked every profile for its history tile before checking it is fused: a profile with no attention cache would
be refused at registration past the fence. **D3.** Tail `0xC2` is RFC-0003's gen classes (RFC-0004 holds `0xC3`–`0xDA`): moved to `0xE0`,
pinned by value, position and decode, disjoint from every tail of this build and spec 17 §17.0; no other number is allocated; delta 90
stays (RFC-0003's shifts on merge). **D4.** `fork_id_v1::set_fence_for_probe` had no arm (`every_scheduled_palw_fence…` panics); 742 lines
of rustfmt churn in `palw_state_v2.rs` and four reformat-only test files dropped (they would conflict with RFC-0003/0004 and lane A).
**D5 (held out).** The `palw_panel.rs` carrier-funding refactor (cap 1→5, independent roots) breaks the panel's source-text tests, disables
V01/V04's stuck-carrier replacement (`stuck_tip = None`) and rescans the UTXO set every 10 s; node-only, not needed by the fence — saved
as `_wip-backups/codex-t12-court-window-1001.panel-funding-only.patch`.

**Risks.** **R1** A model-window class's retention is no longer a fixed horizon: each court inside retention extends it to `now + w`
(each costs a challenger's reservation). **R2** Every such class gets a row even when `w` = 3,000, so all take the extension rule. **R3**
Seat retention liabilities (`palw_seat_note_liability_v1`) are not refreshed by an extension. **R4** `declare-layout` judges at the IR
fence's height: neither fence2 nor the window is modelled offline. No RPC shows a window: the fold logs it (`PALW model court window: class …`).
