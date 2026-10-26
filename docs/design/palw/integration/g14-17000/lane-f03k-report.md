# lane F03k — the outsider's conviction on the kernel route, T05 (2026-10-26)

Branch `claude/g14-f03k` (base `f25c4b176` plus SPINE `60313ccb4`, `9fee3348f`; `ebb8b911f` was already in the base).

## 1. The kernel route's prosecution path (from the implementation)
- **Who:** any registered bond whose free collateral covers the filing fee (`misaka/palw/kernel/src/ledger.rs` `file_proof`).
- **What:** `K::FileProof { accuser, claim, proof }`. It is non-interactive and preceded by `K::SealProof`, the seal-then-reveal for the bounty. Large filings ride the tag-113 chunk lane. The proof is checked against public material by the court (`adjudicate`).
- **When:** while the claim is live, and after Final or a pre-Final default up to `liability_until` (`proof_open`).
- **Limits:**
  - (a) The court scope (ADR-0177 D2) counts dismissals **per accuser per claim**: 4, plus 16 units for segmented claims. Another bond cannot spend the outsider's allowance.
  - (b) The **block's** adjudication budget: runs and court work, with `prosecution_reserve_permille` reserved for proofs. It starts fresh every block, and a filing reserves nothing across blocks.
  - (c) A dismissed filing pays `dismissal_fee_v1(max_court_work)`, which is burned.
- **Session:** none is needed for a lie whose material is public. For withheld material there is a demand: one shared session per (claim, stage, position), joinable up to 64 demanders, with its deadline fixed when it opens. A full join list only loses the late joiner its share. The default and the later proof do not depend on joining.

## 2. Verdict: the TIR-route lockout does not exist on the kernel route
The kernel route has no per-claim challenger slot and no owner reservation queue. So Sybils cannot take a slot first, hold it silently and then refill it. What they can do is saturate a block's court with junk filings. That costs fees, lasts only that block, and is refused free after 4 dismissals per bond per claim.

The three properties are fixed as invariants by the tests in §3. No code change was needed, so ADR-0192 A's shared frontier is not ported.

**Residual (not closed here):** keeping an outsider out for a claim's whole exposure means saturating *every* block, including blocks the outsider's own node mines. Inside a block, the colluding producer chooses the order. The ledger prices that sustained censorship only for OPV classes (`opv_class_economics`: censorship cost must exceed the maximum gain). A **Panel-licensed class has no such registration check.** Proposal: apply the same `censorship_cost > max_gain` relation to Panel classes. That is a rule change and needs a fence, so it is for 統括 to decide.

## 3. Native T05 tests (`consensus/.../g14_kernel_route_e2e/canonical/f03k.rs`)
Setup: one world. The producer and every seat collude on a lie. Two Sybil bonds (colluding harness cards outside the Panel) saturate a block with junk `FileProof`s, stay silent for one block, do it again, and do it a third time right before the outsider files. Each round asserts that every Sybil paid at least one dismissal fee, nothing was convicted and the lifecycle did not move.

The outsider is the post-genesis `NEWCOMER` bond on an IBD node started after the claim. It reads through RPC ops 210/211 and gets the model from a peer.

| Test | What it shows | Result |
|---|---|---|
| `g14_f03k_t05_outsider_convicts_before_final_through_a_crowded_court` | Conviction; producer slashed by the reservation; the outsider gets the bounty; never paid | PASS |
| `g14_f03k_t05_outsider_convicts_after_final_through_a_crowded_court` | Conviction in the liability phase; the state stays Final | PASS |
| `g14_f03k_t05_without_the_outsider_nothing_convicts` | Negative: the junk convicts nothing; the lie finalizes, is rewarded and is not slashed | PASS |

Command: `buildslot.sh env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --offline -p kaspa-consensus --lib g14_f03k`. Exit 0, 3 passed.

**Fixture limits:**
- The canonical `world` assigns a **3-seat** interim Panel (`claimed` asserts 3), not Panel=1.
- The Sybils are harness keys. They are attackers, and nothing here claims they are independent.
- G-K1 seat charges are not asserted.
- The same-block race, where Sybil junk sits before the outsider's filing inside one block, is not exercised. Template order is not controlled.

## 4. Proposal for the spine (S3b' / S3b)
Replace S3b' `Missing` with a `Stage` that runs `f03k::t05(false, true)` (S3b' before Final) and `t05(true, true)` (after Final). That needs `t05` to be `pub(super)`. SPINE owns spine.rs, so I have not edited it.

The F03b reading of S3b' ("a shared frontier is needed on the kernel route") is withdrawn for the kernel route, for the reason in §2.

Panel=1 still needs a 1-seat world fixture (G14C/SPINE).

## 5. Second round (F03k2, branch `claude/g14-f03k2`, base 22b657aa9)
1. **Panel=1 world.**
   - `canonical/f03k.rs::panel_one_config`/`panel_one_world` arm `palw_kernel_route_panel_one_v1` from genesis and mirror it onto the bundle (as `sync_palw_g14_gates_v1` does).
   - `claimed` now asserts the seat count the route's policy gives (`kernel_route_seats_and_quorum_v1`), no longer a fixed 3.
   - T05 runs in this world with 1 colluding seat: before Final, after Final, and negative.
   - `t05(after_final, outsider)` is `pub(super)` and runs in the Panel=1 world, for SPINE's S3b'.
2. **Censorship-cost relation for Panel classes**, inside the dormant `palw_kernel_route_panel_one_v1` (no new fence).
   - Module `consensus/core/src/palw_kernel_route_panel_one_censorship_v1.rs`. The relation is `min(max_adjudications, ⌈block work / worst⌉) × dismissal_fee_v1(worst) × (check + challenge + liability windows) > claim_reward + external gain bound`.
   - Applied in the kernel fold after `ClassRegistered`. A failing class keeps only the spent budget, the same pattern as the carrier-fit refusal.
   - The external gain bound is `palw_g14_values_v1::PALW_G14_KERNEL_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI_V1 = None`, which is typed-missing. Past the fence every Panel class is refused until a value is adopted. Court ingress and V3 do not price a kernel `FileProof`, so the fee is the ledger's actual dismissal fee.
   - Fixtures set the bound through the borsh-skipped `with_test_only_kernel_panel_one_external_gain_bound_v1`. The fixture value is `TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI` = 1 BILI.
   - Tests:
     - refused when the bound is missing;
     - refused when censorship is cheaper than the gain;
     - registered when it is dearer;
     - below the fence, the same root whatever the input. Arming the fence at all changes the hashed params and so the root; that is a property of the existing fence.
3. **Same-block race: NOT shown, a decision item.**
   - `BlockBudgetV1::charged_v1` is first-come within the block.
   - In the test net, one worst-case `FileProof` can spend the whole block's court work.
   - Sybil junk ordered before the outsider's filing in a block that a colluder produces therefore starves it in that block. The refused filing costs nothing, and it gets through in any block the colluders do not fill, for example its own node's.
   - The per-filer cap (4 per claim) does not stop fresh Sybil bonds.
   - Options for 統括: (a) accept this, since the relation in item 2 prices whole-exposure saturation; or (b) order `FileProof`s within a block deterministically (for example, the oldest seal first). (b) needs a fence.
