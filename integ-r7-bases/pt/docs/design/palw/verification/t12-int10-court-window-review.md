# int-10: the model court window — review (2026-10-01)

Subject: Codex's `palw_model_court_window` (`codex/t12-court-window`, uncommitted state of 2026-10-01 09:40), applied to `rcore/int-10`. Past the fence a
fused-attention graph class or an IR class with a dissected cone commits at registration `w = max(network window, W(shape))`,
`W = (2·(L+H) + t + 1)·turn + assembly reserve + 1` (`L` the leaf ladder's rounds, `H` the history dissection's, `t` the terminal moves), stored in
`class_court_windows` (root block, carriage tail `0xE0`, delta 90); each court opened for that class takes `w` as its backstop and extends the claim's
retention to its deadline.

**Decision (coordinator, 2026-10-01): the window stays DORMANT on testnet-12.** The DAA-3,600 flag day is `palw_tir_fence2` alone
(`PALW_T12_TIR_FENCE2_DAA`); `PALW_T12_MODEL_COURT_WINDOW_DAA` is `None`. The code (with the D1 gating), `PALW_T12_MODEL_COURT_WINDOW_FENCES_V1` and
`--palw-drill-model-court-at` stay, armed nowhere, for a ruleset that charges the ladder clock. The int-8 baseline (`palw_t12_release_v4_params`, params
`3db42ea6…`) is the shipped ruleset with fence2 cleared, pinned by value.

**Where 5,102 comes from, and why t12 never charges it.** `palw_attn_court_admits_row_v1` (`palw_attn_court_v1.rs`), the LADDER clock, charges every fused
class `(2·(L+H) + t + root claim)·turn_deadline + reserve` and reports it as `PalwClassAdmissionError::CourtWindowTooShort { needed }`
(`verify_class_admission_v8/v9`): at t12's court (L 40, turn 42, terminal 2, arity 4, reserve 1,616) and a history of at most one tile (H 0) that is
83 × 42 + 1,616 = **5,102 > 3,000**, for every class. testnet-12 never charges it: `held.armed` is the network's `palw_held_context`, armed from genesis,
so admission calls `palw_attn_court_admits_row_held_v1` (no leaf ladder), and the fold refuses `CourtOpened` for every held claim
(`BisectionRefusedUnderHeldContext`). Under the held clock `W` is **≤ 3,000 up to 2^32 positions at tile 16** (2,919 at 2^32; 3,591 at 2^48; 4,263 at
`u64::MAX`): `w` IS the network window for every admissible class.

**Proof** (`consensus/core/tests/palw_t12_court_window_changes_no_admission.rs`; the fence armed and unarmed through its own `set`, each class through the
acceptance path's gate): the same admission shape, window (3,000) and verdict in every case — the floor, hybrid rows at 512 / 4,096 / 32,768 positions and
dense rows at 512 … 2,097,152 (all admitted); a hybrid at 131,072 and the Qwen3.5-9B-shaped one at 262,144 (refused both ways, by the close-bytes wall
`LinearInTheContext`, not by the window); a hybrid and a dense at 2^32 positions (refused both ways, by the A16 history bound 2,097,152); IR classes
(dissected cones, 64 / 4,096 / 32,768 positions) admitted both ways through admission v10 (larger contexts are not constructible). The test fails if the
fence ever changes an admission outcome on t12; the stop condition did not trigger.

**Checks.** One derivation serves admission, the acceptance walk and the fold; all `checked_*`, `L`,`H` ≤ 64 (so `W` ≤ 10,479 at t12), overflow or a zero
tile refused, a missing row fails closed (`ClassCourtWindowMissing`). Reorg: `ClassCourtWindow{key,old,new}` through the same `swap_write!` as every table,
discriminant 90 pinned, round trip tested. IBD: tail and root block only when non-empty, so below the fence (everywhere on t12) both are int-8's byte for
byte; the dormant sync walk (`palw_state_v2_sync.rs`) must carry the fence if ever wired. fence2's DA ladder bounds step LEAVES, the window the TIME of a
session; they meet only in `claim.trace_retention_daa`, untouched while no class has a row. The producer guard `palw_tir_da_answerable_leaves_v1` follows
fence2 (2^22 below, the held network ladder 2^40 from it).

**Defects (fixed).** **D1 (fork risk):** the retention rule (refuse only an opening past retention; extend retention to the deadline) applied to every class
and below the fence — a court int-8 refuses would open on int-10, a split during the rollout; now gated on the class having a row. **D2:** the derivation
asked every profile for its history tile before checking it is fused. **D3:** tail `0xC2` is RFC-0003's gen classes (RFC-0004 holds `0xC3`–`0xDA`): moved to
`0xE0`, pinned by value, position and decode, disjoint from spec 17 §17.0. **D4:** `fork_id_v1::set_fence_for_probe` had no arm; rustfmt churn dropped. **D6:** `verify_class_admission_v10` read the network window from the bundle instead of the
rules it was handed — identical in production, but a window of 1 DAA admitted every dissected cone (`palw_tir_dissect_admission` red); now the rules' own
`window_court_daa`.
**D5 (held out):** the `palw_panel.rs` carrier-funding refactor breaks the panel's source-text tests and disables V01/V04's stuck-carrier replacement;
node-only, not needed — saved as `_wip-backups/codex-t12-court-window-1001.panel-funding-only.patch`.

**Risks (only for a ruleset that arms it).** R1 a model-window class's retention is no longer a fixed horizon (each court inside retention extends it to
`now + w`); R2 every such class gets a row even when `w` = 3,000; R3 seat retention liabilities are not refreshed by an extension; R4 `declare-layout`
judges at the IR fence's height: neither fence2 nor the window is modelled offline.
